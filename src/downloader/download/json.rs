use std::path::PathBuf;

use anyhow::Context;
use indicatif::{ProgressBar, ProgressStyle};

use crate::downloader::Downloader;

impl Downloader {
    pub async fn download_file_pb(
        &self,
        url: &str,
        output_path: &PathBuf,
        pb: Option<&ProgressBar>,
    ) -> anyhow::Result<()> {
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
