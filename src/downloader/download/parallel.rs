use std::{path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use futures::{StreamExt, stream};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use tidlers::{TidalClient, client::models::track::Track};

use crate::{
    downloader::{
        Downloader, context::AlbumTagContext, rate_limiter::RateLimitState,
        ui::summary::DownloadSummary,
    },
    types::MediaType,
};

impl Downloader {
    pub async fn download_tracks_parallel(
        &self,
        client: &mut TidalClient,
        tracks: Vec<Track>,
        output_dir: &PathBuf,
        _use_index_as_track_number: bool,
        album_context: Option<AlbumTagContext>,
        media_type: MediaType,
    ) -> Result<DownloadSummary> {
        println!(
            "\ndownloading {} tracks in parallel (max {})...\n",
            tracks.len(),
            self.max_parallel
        );

        let downloader = Arc::new(self);
        let client = Arc::new(tokio::sync::Mutex::new(client));
        let rate_limit_state = RateLimitState::new();

        // Create multi-progress bar
        let multi_progress = MultiProgress::new();
        rate_limit_state
            .set_multi_progress(multi_progress.clone())
            .await;

        let results = stream::iter(tracks.into_iter().enumerate())
            .map(async |(_, track)| {
                let downloader = Arc::clone(&downloader);
                let client = Arc::clone(&client);
                let output_dir = output_dir.clone();
                let album_context = album_context.clone();
                let rate_limit_state = Arc::clone(&rate_limit_state);
                let multi_progress = multi_progress.clone();
                let mut attempt = 0;
                let max_attempts = 10;

                loop {
                    // Wait if rate limited BEFORE creating progress bar
                    rate_limit_state.wait_if_rate_limited().await;

                    let pb = multi_progress.add(ProgressBar::new_spinner());
                    pb.set_style(
                        ProgressStyle::default_spinner()
                            .template("{spinner:.green} [{elapsed_precise}] {msg}")
                            .unwrap(),
                    );
                    pb.set_message(format!("{}", track.title));

                    let track_id = track.id.to_string();
                    let result = {
                        let client_guard = client.lock().await;
                        client_guard
                            .get_track_postpaywall_playback_info(track_id, None)
                            .await
                    };

                    match result {
                        Ok(playback_info) => {
                            rate_limit_state.on_success().await;

                            let result = downloader
                                .download_track_with_info_pb(
                                    &track,
                                    &playback_info,
                                    &output_dir,
                                    album_context.clone(),
                                    Some(&pb),
                                    media_type
                                )
                                .await;

                            if result.is_ok() {
                                match result.as_ref().unwrap() {
                                    // true => pb.finish_with_message(format!("✓ {}", track.title)),
                                    true => pb.finish(),
                                    false => pb.finish_with_message(format!("○ {}", track.title)),
                                }
                            } else {
                                // pb.finish_with_message(format!(
                                //     "✗ {} (attempt {}/{})",
                                //     track.title,
                                //     attempt + 1,
                                //     max_attempts
                                // ));
                                pb.set_message(format!(
                                    "✗ {} (attempt {}/{}, retrying...)",
                                    track.title,
                                    attempt + 1,
                                    max_attempts
                                ));
                                if attempt < max_attempts {
                                    attempt += 1;
                                    // notify rate limit state of error
                                    rate_limit_state.on_error().await;

                                    continue;
                                } else {
                                    // notify rate limit state of error
                                    rate_limit_state.on_error().await;
                                }
                            }

                            return (track.title, result);
                        }
                        Err(e) => {
                            pb.finish_with_message(format!(
                                "✗ {} (attempt {}/{}, couldn't get playback info, retrying later...)",
                                track.title,
                                attempt + 1,
                                max_attempts
                            ));

                            if attempt < max_attempts {
                                attempt += 1;

                                // Notify rate limit state of error
                                rate_limit_state.on_error().await;

                                continue;
                            } else {
                                // Notify rate limit state of error
                                rate_limit_state.on_error().await;

                                return (
                                    track.title,
                                    Err(e).context("Failed to get playback info"),
                                );
                            }
                        }
                    }
                }
            })
            .buffer_unordered(self.max_parallel)
            .collect::<Vec<_>>()
            .await;

        Ok(DownloadSummary::from_results(results))
    }
}
