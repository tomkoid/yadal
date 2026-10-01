use anyhow::{Context, Result};
use indicatif::ProgressBar;
use std::path::Path;
use tidlers::client::models::track::{
    Track,
    playback::{ManifestType, TrackPlaybackInfoResponse},
};

use crate::{
    config::{LyricsMode, ReplayGainMode},
    downloader::{
        Downloader,
        config::DownloaderConfigTags,
        context::{AlbumTagContext, ReplayGainValues, TagLyrics, TagReplayGain, TrackTagMetadata},
    },
    types::MediaType,
};

pub mod dash;
pub mod json;

#[derive(Debug, Clone)]
pub struct QueuedTrack {
    pub track: Track,
    pub index: usize,
}

pub struct DownloadTrackRequest<'a> {
    pub track: &'a Track,
    pub playback_info: &'a TrackPlaybackInfoResponse,
    pub output_path: &'a Path,
    pub album_context: Option<AlbumTagContext>,
    pub index: Option<usize>,
    pub pb: Option<&'a ProgressBar>,
    pub media_type: MediaType,
}

impl Downloader {
    /// Downloads one track from playback info and tags the resulting file.
    pub async fn download_track_with_info_pb(
        &self,
        request: DownloadTrackRequest<'_>,
    ) -> Result<()> {
        let output_path = request
            .output_path
            .join(request.track.title.clone())
            .with_extension(Self::get_file_extension(&request.playback_info));

        match &request.playback_info.manifest_parsed {
            Some(ManifestType::Dash(dash)) => {
                self.download_dash_track_pb(dash, &output_path, &request.track.title, request.pb)
                    .await?;
            }
            Some(ManifestType::Json(json_manifest)) => {
                if let Some(url) = json_manifest.urls.first() {
                    self.download_file_pb(url, &output_path, request.pb).await?;
                } else {
                    anyhow::bail!("No URLs in manifest");
                }
            }
            None => {
                anyhow::bail!("No parsed manifest available");
            }
        }

        let temp_output_path = match self.config.download.skip_transcode {
            true => output_path,
            false => {
                self.maybe_convert_flac_container(&output_path, request.playback_info)
                    .await?
            }
        };

        if self.config.tags.enable {
            let tag_metadata = self.make_full_track_tags(&request).await;
            let configured_tag_metadata =
                Self::apply_config_to_tags(&self.config.tags, tag_metadata).await;

            self.tag_downloaded_file(&temp_output_path, &configured_tag_metadata)
                .await
                .context("Failed to tag downloaded file")?;
        }

        Ok(())
    }

    async fn make_full_track_tags(&self, request: &DownloadTrackRequest<'_>) -> TrackTagMetadata {
        let mut tm = TrackTagMetadata::from_track(request.track, &request.album_context);

        // handle lyrics
        'once: {
            let lyrics_res = match self
                .tidal_client
                .get_track_lyrics(request.track.id.to_string())
                .await
            {
                Ok(lyrics_res) => lyrics_res,
                Err(_) => break 'once,
            };

            tm.lyrics = if let Some(synced_lyrics) = lyrics_res.subtitles {
                TagLyrics::UnsyncedAndSynced(lyrics_res.lyrics, synced_lyrics)
            } else {
                TagLyrics::UnsyncedOnly(lyrics_res.lyrics)
            };
        }

        // handle ReplayGain
        let rp_gain_track_values = if let Some(gain) =
            fmt_gain(request.playback_info.track_replay_gain)
            && let Some(peak) = fmt_peak(request.playback_info.track_peak_amplitude)
        {
            Some(ReplayGainValues { gain, peak })
        } else {
            None
        };

        let rp_gain_album_values = if let Some(gain) =
            fmt_gain(request.playback_info.album_replay_gain)
            && let Some(peak) = fmt_peak(request.playback_info.album_peak_amplitude)
        {
            Some(ReplayGainValues { gain, peak })
        } else {
            None
        };

        tm.replaygain = if let Some(ref rp_gain_track_values) = rp_gain_track_values
            && let Some(rp_gain_album_values) = rp_gain_album_values
        {
            TagReplayGain::TrackAndAlbum(rp_gain_track_values.clone(), rp_gain_album_values)
        } else if let Some(rp_gain_track_values) = rp_gain_track_values {
            TagReplayGain::TrackOnly(rp_gain_track_values.clone())
        } else if let Some(rp_gain_album_values) = rp_gain_album_values {
            TagReplayGain::AlbumOnly(rp_gain_album_values)
        } else {
            TagReplayGain::None
        };
        tm
    }

    // give the tags to the function and then get it back
    async fn apply_config_to_tags(
        tag_config: &DownloaderConfigTags,
        mut tag_metadata: TrackTagMetadata,
    ) -> TrackTagMetadata {
        let t = tag_config;
        let tm = &mut tag_metadata;

        tm.tag_title = t.title;
        if !t.album {
            tm.album_title = None
        }
        if !t.album_artist {
            tm.album_artists = None
        }
        if !t.artist {
            tm.artists = None
        }
        if !t.bpm {
            tm.bpm = None
        }
        if !t.copyright {
            tm.copyright = None
        }
        if !t.cover {
            tm.cover_url = None
        }
        if !t.date {
            tm.release_date = None
        }
        if !t.disc_number {
            tm.disc_number = None
        }
        if !t.initial_key_and_key_scale {
            tm.key = None;
            tm.key_scale = None;
        }
        if !t.isrc {
            tm.isrc = None
        }
        if !t.total_discs {
            tm.total_discs = None
        }
        if !t.total_tracks {
            tm.total_tracks = None
        }
        if !t.track_number {
            tm.track_number = None
        }
        if !t.track_version {
            tm.track_version = None
        }
        if !t.url {
            tm.url = None
        }

        // end the mutable borrow to avoid having to clone in the matches below
        let tm = ();
        // use tm so the compiler doesn't complain about an unused variable
        let _ = tm;

        // the make_track_tags() method doesn't return options with the context of the tagging config
        // these two sections for the lyrics and ReplayGain apply the context of the config to the returned value

        tag_metadata.lyrics = match t.lyrics {
            LyricsMode::UnsyncedOnly => match tag_metadata.lyrics {
                TagLyrics::UnsyncedAndSynced(unsynced, _) => TagLyrics::UnsyncedOnly(unsynced),
                _ => tag_metadata.lyrics,
            },
            LyricsMode::SyncedOnly => match tag_metadata.lyrics {
                TagLyrics::UnsyncedAndSynced(_, lyrics) => TagLyrics::SyncedOnly(lyrics),
                _ => tag_metadata.lyrics,
            },
            _ => tag_metadata.lyrics,
        };

        tag_metadata.replaygain = match t.replaygain {
            ReplayGainMode::TrackOnly => match tag_metadata.replaygain {
                TagReplayGain::TrackAndAlbum(track_values, _) => {
                    TagReplayGain::TrackOnly(track_values)
                }
                _ => tag_metadata.replaygain,
            },
            ReplayGainMode::AlbumOnly => match tag_metadata.replaygain {
                TagReplayGain::TrackAndAlbum(_, album_values) => {
                    TagReplayGain::AlbumOnly(album_values)
                }
                _ => tag_metadata.replaygain,
            },
            _ => tag_metadata.replaygain,
        };

        tag_metadata
    }
}

fn fmt_gain(db: f64) -> Option<String> {
    db.is_finite().then(|| format!("{:.2} dB", db))
}

fn fmt_peak(amp: f64) -> Option<String> {
    amp.is_finite().then(|| format!("{:.6}", amp))
}
