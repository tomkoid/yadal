use anyhow::{Context, Result};
use indicatif::ProgressBar;
use std::path::Path;
use tidlers::client::models::track::{
    Track,
    playback::{ManifestType, TrackPlaybackInfoResponse},
};

use crate::{
    downloader::{
        Downloader,
        context::{AlbumTagContext, TrackTagMetadata},
    },
    types::MediaType,
};

pub mod dash;
pub mod json;
pub mod parallel;

impl Downloader {
    /// Returns Ok(true) if the track was downloaded, Ok(false) if it was skipped due to existing
    /// file, or Err on failure
    pub async fn download_track_with_info_pb(
        &self,
        track: &Track,
        playback_info: &TrackPlaybackInfoResponse,
        output_dir: &Path,
        album_context: Option<AlbumTagContext>,
        index: Option<usize>,
        pb: Option<&ProgressBar>,
        media_type: MediaType,
    ) -> Result<bool> {
        let extension = self.get_file_extension(playback_info);
        let base_name = self.get_track_base_name(track, &media_type, index);
        let output_path = output_dir.join(format!("{}.{}", base_name, extension));

        match &playback_info.manifest_parsed {
            Some(ManifestType::Dash(dash)) => {
                self.download_dash_track_pb(dash, &output_path, &track.title, pb)
                    .await?;
            }
            Some(ManifestType::Json(json_manifest)) => {
                if let Some(url) = json_manifest.urls.first() {
                    self.download_file_pb(url, &output_path, pb).await?;
                } else {
                    anyhow::bail!("No URLs in manifest");
                }
            }
            None => {
                anyhow::bail!("No parsed manifest available");
            }
        }

        let output_path = self
            .maybe_convert_flac_container(&output_path, playback_info)
            .await?;

        let tag_metadata = TrackTagMetadata::from_track(track, album_context);
        self.tag_downloaded_file(&output_path, &tag_metadata)
            .await
            .context("Failed to tag downloaded file")?;

        Ok(true) // file was downloaded
    }
}
