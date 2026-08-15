use std::path::PathBuf;

use crate::downloader::Downloader;
use anyhow::{Context, Result};
use bytes::Bytes;
use futures::{StreamExt, stream};
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::{StatusCode, header};
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
                    .template("{spinner} [{bar:10.magenta/blue}] {pos:3}/{len:3} segments (eta {eta}) [{elapsed}]\t{wide_msg}")
                    .unwrap()
                    .progress_chars("#>-"),
            );
        }

        // download initialization segment for dash
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

        // discover segment count so progress total is accurate
        let total_segments = self
            .estimate_dash_total_segments(dash, track_title, pb)
            .await;
        if total_segments == 0 {
            anyhow::bail!("No downloadable DASH segments found");
        }

        if let Some(pb) = pb {
            pb.set_length(total_segments as u64);
            pb.set_position(0);
        }

        // download all segments using the discovered total
        let mut all_segments: Vec<(u32, Bytes)> = Vec::new();
        let mut segment_num = 1;
        let batch_size = 50; // 50 segments at a time

        while segment_num <= total_segments {
            let batch_end = (segment_num + batch_size - 1).min(total_segments);

            // prepare batch of segment URLs
            let mut batch_urls = Vec::new();
            for current_num in segment_num..=batch_end {
                let Some(url) = dash.get_segment_url(current_num) else {
                    anyhow::bail!("Missing segment URL for segment {current_num}");
                };
                batch_urls.push((current_num, url));
            }

            if let Some(pb) = pb {
                pb.set_message(format!(
                    "{track_title}: Downloading segments {}-{}...",
                    segment_num, batch_end
                ));
            }

            // download batch in parallel
            let batch_results = stream::iter(batch_urls)
                .map(|(num, url)| async move {
                    const SEGMENT_RETRIES: u32 = 3;
                    for attempt in 1..=SEGMENT_RETRIES {
                        match self.download_segment(&url).await {
                            Ok(data) => return Ok((num, data)),
                            Err(e) => {
                                if attempt == SEGMENT_RETRIES {
                                    return Err((num, e));
                                }

                                let backoff_ms = 150 * attempt;
                                tokio::time::sleep(std::time::Duration::from_millis(
                                    backoff_ms.into(),
                                ))
                                .await;
                            }
                        }
                    }
                    unreachable!("segment retry loop must return");
                })
                .buffer_unordered(20)
                .collect::<Vec<_>>()
                .await;

            // Check results.
            let mut batch_segments = Vec::new();

            for result in batch_results {
                match result {
                    Ok((num, data)) => {
                        batch_segments.push((num, data));
                        if let Some(pb) = pb {
                            pb.inc(1);
                            pb.set_message(format!("{track_title}: Downloaded segment {}", num));
                        }
                    }
                    Err((num, e)) => {
                        if let Some(pb) = pb {
                            pb.set_message(format!(
                                "{track_title}: Failed to download segment {}",
                                num
                            ));
                        }
                        return Err(anyhow::anyhow!("Failed to download segment {num}: {e:#}"));
                    }
                }
            }

            all_segments.extend(batch_segments);
            segment_num = batch_end + 1;
        }

        if let Some(pb) = pb {
            pb.set_message(format!("{track_title}: Combining segments..."));
        }

        // sort and combine
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

        std::fs::write(output_path, combined_data).context("Failed to write file")?;

        if let Some(pb) = pb {
            pb.finish_with_message(format!("{track_title}: Downloaded"));
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

    async fn estimate_dash_total_segments(
        &self,
        dash: &DashManifest,
        track_title: &str,
        pb: Option<&ProgressBar>,
    ) -> u32 {
        const MAX_SEGMENT_PROBE: u32 = 20_000;

        if let Some(pb) = pb {
            pb.set_message(format!("{track_title}: Discovering total segments..."));
        }

        if !self.segment_exists(dash, 1).await {
            return 0;
        }

        let mut low = 1_u32;
        let mut high = 2_u32;

        while high < MAX_SEGMENT_PROBE && self.segment_exists(dash, high).await {
            low = high;
            high = (high.saturating_mul(2)).min(MAX_SEGMENT_PROBE);
        }

        if high <= low {
            return low;
        }

        let mut left = low + 1;
        let mut right = high;

        while left <= right {
            let mid = left + (right - left) / 2;
            if self.segment_exists(dash, mid).await {
                low = mid;
                left = mid + 1;
            } else if mid == 0 {
                break;
            } else {
                right = mid - 1;
            }
        }

        low
    }

    async fn segment_exists(&self, dash: &DashManifest, segment_num: u32) -> bool {
        let Some(url) = dash.get_segment_url(segment_num) else {
            return false;
        };

        self.segment_url_exists(&url).await
    }

    async fn segment_url_exists(&self, url: &str) -> bool {
        let head_res = self
            .http_client
            .head(url)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await;

        match head_res {
            Ok(resp) if resp.status().is_success() => return true,
            Ok(resp)
                if resp.status() == StatusCode::NOT_FOUND || resp.status() == StatusCode::GONE =>
            {
                return false;
            }
            _ => {}
        }

        let get_res = self
            .http_client
            .get(url)
            .header(header::RANGE, "bytes=0-0")
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await;

        match get_res {
            Ok(resp) => resp.status().is_success() || resp.status() == StatusCode::PARTIAL_CONTENT,
            Err(_) => false,
        }
    }
}
