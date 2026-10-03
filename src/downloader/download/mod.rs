use anyhow::{Context, Result, bail};
use indicatif::ProgressBar;
use tidlers::{
    client::models::track::{
        Track,
        playback::{ManifestType, TrackPlaybackInfoResponse},
    },
    resources::uuid_to_url_with_size,
};

use crate::{
    config::{LyricsMode, ReplayGainMode},
    downloader::{
        Downloader,
        config::DownloaderConfigTags,
        context::{
            AlbumTagContext, PlaylistTemplateContext, ReplayGainValues, TagLyrics, TagReplayGain,
            TrackTagMetadata,
        },
        template::{Templater, render_track_path},
    },
};

pub mod dash;
pub mod json;

#[derive(Debug, Clone)]
pub struct TrackJob {
    pub track: Track,
    pub album: AlbumTagContext,
    pub playlist: Option<PlaylistTemplateContext>,
    /// 1-based position inside the album/playlist (1 for standalone tracks)
    pub position: usize,
}

pub enum TrackOutcome {
    Downloaded,
    Skipped,
}

impl Downloader {
    pub async fn process_track(
        &self,
        job: &TrackJob,
        playback_info: &TrackPlaybackInfoResponse,
        templater: &Templater,
        pb: Option<&ProgressBar>,
    ) -> Result<TrackOutcome> {
        let skip_transcode = self.config.download.skip_transcode;
        let raw_ext = Self::get_file_extension(playback_info);
        let remux = !skip_transcode && Self::needs_flac_remux(playback_info, raw_ext);
        let final_ext = if remux { "flac" } else { raw_ext };

        let relative = render_track_path(
            templater,
            &job.track,
            &job.album,
            job.playlist.as_ref().map(|p| (p, job.position)),
            final_ext,
        )
        .context("failed to render output template")?;

        let final_path = self.config.download.output_path.join(relative);

        if !self.config.download.force_download && final_path.try_exists().unwrap_or(false) {
            if let Some(pb) = pb {
                pb.finish_with_message(format!("skipped {} (already exists)", job.track.title));
            }
            return Ok(TrackOutcome::Skipped);
        }

        if let Some(parent) = final_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut part_name = final_path.as_os_str().to_owned();
        part_name.push(".part");

        let part_path = std::path::PathBuf::from(part_name);

        match &playback_info.manifest_parsed {
            Some(ManifestType::Dash(dash)) => {
                self.download_dash_track_pb(dash, &part_path, &job.track.title, pb)
                    .await?;
            }
            Some(ManifestType::Json(json_manifest)) => {
                let url = json_manifest.urls.first().context("no URLs in manifest")?;
                self.download_file_pb(url, &part_path, pb).await?;
            }
            None => bail!("No parsed manifest available"),
        }

        if remux {
            Self::remux_to_flac(part_path.clone(), final_path.clone()).await?;
            std::fs::remove_file(&part_path)?;
        } else {
            std::fs::rename(&part_path, &final_path)?;
        }

        if self.config.tags.enable {
            let tag_metadata = self.make_full_track_tags(job, playback_info).await;
            let configured = Self::apply_config_to_tags(&self.config.tags, tag_metadata).await;

            let log = |msg: String| match pb {
                Some(pb) if !pb.is_hidden() => pb.println(format!("  {msg}")),
                _ => eprintln!("  {msg}"),
            };

            self.tag_downloaded_file(&final_path, &configured, &log)
                .await
                .context("Failed to tag downloaded file")?;
        }

        Ok(TrackOutcome::Downloaded)
    }

    async fn make_full_track_tags(
        &self,
        job: &TrackJob,
        playback_info: &TrackPlaybackInfoResponse,
    ) -> TrackTagMetadata {
        let mut tm = TrackTagMetadata::from_track(&job.track, &job.album);

        if let Some(cover_url) = job
            .album
            .cover_uuid
            .as_deref()
            .or_else(|| job.track.album.as_ref().and_then(|a| a.cover.as_deref()))
            .map(|uuid| uuid_to_url_with_size(uuid, 1280))
            && let Ok(cover_image) = self.fetch_cover_picture(&cover_url).await
        {
            tm.cover = Some(cover_image);
        }

        // handle lyrics
        'once: {
            let lyrics_res = match self
                .tidal_client
                .get_track_lyrics(job.track.id.to_string())
                .await
            {
                Ok(lyrics_res) => lyrics_res,
                Err(_) => break 'once,
            };

            tm.lyrics = if let Some(ref unsynced_lyrics) = lyrics_res.lyrics
                && let Some(ref synced_lyrics) = lyrics_res.subtitles
            {
                TagLyrics::UnsyncedAndSynced(unsynced_lyrics.to_owned(), synced_lyrics.to_owned())
            } else if let Some(ref unsynced_lyrics) = lyrics_res.lyrics {
                TagLyrics::UnsyncedOnly(unsynced_lyrics.to_owned())
            } else if let Some(ref synced_lyrics) = lyrics_res.subtitles {
                TagLyrics::SyncedOnly(synced_lyrics.to_owned())
            } else {
                TagLyrics::None
            }
        }

        // handle ReplayGain
        let rp_gain_track_values = if let Some(gain) = fmt_gain(playback_info.track_replay_gain)
            && let Some(peak) = fmt_peak(playback_info.track_peak_amplitude)
        {
            Some(ReplayGainValues { gain, peak })
        } else {
            None
        };

        let rp_gain_album_values = if let Some(gain) = fmt_gain(playback_info.album_replay_gain)
            && let Some(peak) = fmt_peak(playback_info.album_peak_amplitude)
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
            tm.cover = None
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
        #[allow(unused)]
        let tm = ();

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
