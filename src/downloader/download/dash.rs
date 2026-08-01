use std::path::PathBuf;

use crate::downloader::Downloader;
use anyhow::{Context, Result};
use bytes::Bytes;
use futures::{StreamExt, stream};
use indicatif::{ProgressBar, ProgressStyle};
use tidlers::client::models::track::playback::DashManifest;

impl Downloader {
    pub async fn download_dash_track_pb(
        &self,
        dash: &DashManifest,
        output_path: &PathBuf,
        track_title: &str,
        pb: Option<&ProgressBar>,
    ) -> Result<()> {
        if let Some(pb) = pb {
            pb.set_length(1);
            pb.set_position(0);
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} segments\t{msg} (eta {eta})")
                    .unwrap()
                    .progress_chars("#>-"),
            );
        }

        // Step 1: Download initialization segment (required for DASH)
        let init_data = if let Some(init_url) = dash.get_init_url() {
            if let Some(pb) = pb {
                pb.set_message(format!("{track_title}: Downloading init segment..."));
            }
            self.download_segment(init_url).await?
        } else {
            if let Some(pb) = pb {
                pb.set_message("{track_title}: No initialization segment found, skipping...");
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }

            anyhow::bail!("No initialization segment found");
        };

        // Step 2: Download segments with adaptive discovery
        // We'll download in batches and stop when we hit consecutive failures
        let mut all_segments: Vec<(u32, Bytes)> = Vec::new();
        let mut segment_num = 1;
        let batch_size = 50; // Download 50 segments at a time

        loop {
            // Prepare batch of segment URLs
            let mut batch_urls = Vec::new();
            for i in 0..batch_size {
                if let Some(url) = dash.get_segment_url(segment_num + i) {
                    batch_urls.push((segment_num + i, url));
                } else {
                    break;
                }
            }

            if batch_urls.is_empty() {
                break;
            }

            if let Some(pb) = pb {
                let known_total = (segment_num - 1) as u64 + batch_urls.len() as u64;
                if pb.length().unwrap_or(0) < known_total {
                    pb.set_length(known_total);
                }
                pb.set_message(format!(
                    "{track_title}: Downloading segments {}-{}...",
                    segment_num,
                    segment_num + batch_urls.len() as u32 - 1
                ));
            }

            // Download batch in parallel
            let batch_results = stream::iter(batch_urls)
                .map(|(num, url)| async move {
                    match self.download_segment(&url).await {
                        Ok(data) => Ok((num, data)),
                        Err(e) => Err((num, e)),
                    }
                })
                .buffer_unordered(20)
                .collect::<Vec<_>>()
                .await;

            // Check results
            const MAX_CONSECUTIVE_FAILURES: u32 = 1000;
            let mut consecutive_failures = 0;
            let mut batch_segments = Vec::new();

            for result in batch_results {
                match result {
                    Ok((num, data)) => {
                        batch_segments.push((num, data));
                        consecutive_failures = 0;
                        if let Some(pb) = pb {
                            pb.inc(1);
                            pb.set_message(format!("{track_title}: Downloaded segment {}", num));
                        }
                    }
                    Err(_) => {
                        consecutive_failures += 1;
                        if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                            if let Some(pb) = pb {
                                pb.set_message(format!(
                                    "{track_title}: Downloading FAILED on segment {segment_num}, failures: {consecutive_failures}/{MAX_CONSECUTIVE_FAILURES}",
                                ));
                            }
                            return Err(anyhow::anyhow!(
                                "Failed to download segment {segment_num} after {consecutive_failures} consecutive failures"
                            ));
                            // break;
                        }

                        if let Some(pb) = pb {
                            pb.set_message(format!(
                                "{track_title}: Downloading segment {segment_num}, failures: {consecutive_failures}/{MAX_CONSECUTIVE_FAILURES}",
                            ));
                        }

                        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
                    }
                }
            }

            // If we got no segments in this batch, we're done
            if batch_segments.is_empty() {
                break;
            }

            all_segments.extend(batch_segments);
            segment_num += batch_size;

            // Stop if we hit too many failures
            if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                break;
            }
        }

        if let Some(pb) = pb {
            let downloaded_segments = pb.position();
            pb.set_length(downloaded_segments.max(1));
            pb.set_position(downloaded_segments);
            pb.set_message(format!("{track_title}: Combining segments..."));
        }

        // Step 3: Sort and combine
        all_segments.sort_by_key(|(num, _)| *num);

        let total_size = init_data.len()
            + all_segments
                .iter()
                .map(|(_, data)| data.len())
                .sum::<usize>();
        let mut combined_data = Vec::with_capacity(total_size);
        combined_data.extend_from_slice(&init_data);

        for (_, segment_data) in all_segments {
            combined_data.extend_from_slice(&segment_data);
        }

        if let Some(pb) = pb {
            pb.set_message(format!("{track_title}: Writing to disk..."));
        }

        // Write to file
        std::fs::write(output_path, combined_data).context("Failed to write file")?;

        if let Some(pb) = pb {
            pb.set_message(format!("{track_title}: Saved successfully."));
        }

        Ok(())
    }
    pub async fn download_segment(&self, url: &str) -> Result<Bytes> {
        let response = self
            .http_client
            .get(url)
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await
            .context("Failed to send request")?;

        if !response.status().is_success() {
            anyhow::bail!("HTTP {}", response.status());
        }

        response.bytes().await.context("Failed to read bytes")
    }
}
