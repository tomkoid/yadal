use anyhow::{Context, Result};
use indicatif::ProgressBar;
use std::path::Path;
use tidlers::client::models::track::{
    Track,
    playback::{ManifestType, TrackPlaybackInfoResponse},
};

use crate::{
    config::{LyricsMode, ReplayGainMode}, downloader::{
        Downloader, context::{AlbumTagContext, TagLyrics, TagReplayGain, TrackTagMetadata},
    }, types::MediaType,
};

pub mod dash;
pub mod json;
pub mod parallel;

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
        let extension = self.get_file_extension(request.playback_info);
        let base_name = self.get_track_base_name(request.track, &request.media_type, request.index);
        let output_path = request
            .output_path
            .join(format!("{}.{}", base_name, extension));

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

        let output_path = match self.config.download.skip_transcode {
            true => output_path,
            false => {
                self.maybe_convert_flac_container(&output_path, request.playback_info)
                    .await?
            }
        };

        // exit here so that the tags section is less indented
        if !self.config.tags.enable {
            return Ok(());
        }

        let mut tag_metadata = TrackTagMetadata::from_track(request.track, request.album_context);

        let t = &self.config.tags;let tm = &mut tag_metadata;

        tm.tag_title = self.config.tags.title;
        if !t.album { tm.album_title = None };
        if !t.album_artist { tm.album_artist = None };
        if !t.artist { tm.artists = None};
        if !t.bpm { tm.bpm = None };
        if !t.copyright { tm.copyright = None};
        if !t.cover { tm.cover_url = None};
        if !t.date { tm.release_date = None};
        if !t.disc_number { tm.disc_number = None};
        if !t.initial_key_and_key_scale {
            tm.key = None;
            tm.key_scale = None;
        };
        if !t.isrc { tm.isrc = None};
        if t.lyrics == LyricsMode::None { tm.lyrics = TagLyrics::None };
        if t.replaygain == ReplayGainMode::None { tm.replaygain = TagReplayGain::None};
        if !t.total_discs { tm.total_discs = None};
        if !t.total_tracks { tm.total_tracks = None};
        if !t.url { tm.url = None};

        // handle lyrics
        'once: {
            if !matches!(t.lyrics, LyricsMode::None) {
                let lyrics = match self
                    .tidal_client
                    .get_track_lyrics(request.track.id.to_string())
                    .await {
                        Ok(lyrics_res) => lyrics_res,
                        Err(_) => break 'once
                    };

                match t.lyrics {
                    LyricsMode::UnsyncedOnly => tag_metadata.lyrics = TagLyrics::UnsyncedOnly(lyrics.lyrics),
                    LyricsMode::SyncedOnly => {
                        if let Some(synced_lyrics) = lyrics.subtitles {
                            tag_metadata.lyrics = TagLyrics::SyncedOnly(synced_lyrics);
                        }
                    },
                    LyricsMode::UnsyncedAndSynced => {
                        if let Some(synced_lyrics) = lyrics.subtitles {
                            tag_metadata.lyrics = TagLyrics::UnsyncedAndSynced(lyrics.lyrics, synced_lyrics);
                        } else { // don't fail if we can't get synced lyrics
                            tag_metadata.lyrics = TagLyrics::UnsyncedOnly(lyrics.lyrics);
                        }
                    },
                    _ => unreachable!(),
                }
            }
        }

        self.tag_downloaded_file(&output_path, &tag_metadata)
            .await
            .context("Failed to tag downloaded file")?;

        Ok(())
    }
}
