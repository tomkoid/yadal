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

        let output_path = self
            .maybe_convert_flac_container(&output_path, request.playback_info)
            .await?;

        if !self.config.skip_tag {
            let tag_metadata = TrackTagMetadata::from_track(request.track, request.album_context);
            self.tag_downloaded_file(&output_path, &tag_metadata)
                .await
                .context("Failed to tag downloaded file")?;
        }

        Ok(())
    }
}
