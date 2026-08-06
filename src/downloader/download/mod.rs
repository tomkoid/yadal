use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use std::path::PathBuf;
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
pub mod parallel;

impl Downloader {
    /// Returns Ok(true) if the track was downloaded, Ok(false) if it was skipped due to existing
    /// file, or Err on failure
    pub async fn download_track_with_info_pb(
        &self,
        track: &Track,
        playback_info: &TrackPlaybackInfoResponse,
        output_dir: &PathBuf,
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

    async fn download_file_pb(
        &self,
        url: &str,
        output_path: &PathBuf,
        pb: Option<&ProgressBar>,
    ) -> Result<()> {
        use futures::StreamExt;

        let response = self
            .http_client
            .get(url)
            .timeout(std::time::Duration::from_secs(60))
            .send()
            .await
            .context("Failed to send request")?;

        if !response.status().is_success() {
            anyhow::bail!("HTTP {}", response.status());
        }

        let total_size = response.content_length().unwrap_or(0);

        if let Some(pb) = pb {
            if total_size > 0 {
                pb.set_length(total_size);
                pb.set_style(
                    ProgressStyle::default_bar()
                    .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
                    .unwrap()
                    .progress_chars("#>-")
                );
            }
        }

        let mut downloaded: u64 = 0;
        let mut stream = response.bytes_stream();
        let mut file_data = Vec::new();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("Failed to read chunk")?;
            file_data.extend_from_slice(&chunk);
            downloaded += chunk.len() as u64;

            if let Some(pb) = pb {
                pb.set_position(downloaded);
            }
        }

        if let Some(pb) = pb {
            pb.set_message("Writing to disk...");
        }

        std::fs::write(output_path, file_data).context("Failed to write file")?;

        Ok(())
    }
}
