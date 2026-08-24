use std::{path::Path, sync::Arc, time::Duration};

use anyhow::{Context, Result};
use futures::{StreamExt, stream};
use indicatif::{ProgressBar, ProgressStyle};
use tidlers::{TidalError, client::models::track::config::TrackPlaybackInfoConfig};

use crate::{
    downloader::{
        Downloader,
        context::AlbumTagContext,
        download::{DownloadTrackRequest, QueuedTrack},
        rate_limiter::RateLimitState,
        ui::summary::DownloadSummary,
    },
    types::MediaType,
};

impl Downloader {
    pub async fn download_tracks_parallel(
        &self,
        output_dir: &Path,
        album_context: Option<AlbumTagContext>,
        media_type: MediaType,
    ) -> Result<DownloadSummary> {
        println!(
            "\ndownloading {} tracks in parallel (max {})...",
            self.state.queued.len(),
            self.config.max_parallel
        );

        let downloader = Arc::new(self);
        let client = Arc::new(tokio::sync::Mutex::new(self.tidal_client.clone()));
        let rate_limit_state = RateLimitState::new();

        // Create multi-progress bar
        rate_limit_state
            .set_multi_progress(self.state.multi_progress.as_ref().unwrap().clone())
            .await;

        // instantiate status line
        self.update_finished(0).await;

        let results = stream::iter(self.state.queued.clone())
            .map(|queued_track| {
                let QueuedTrack { track, index } = queued_track;
                let downloader = Arc::clone(&downloader);
                let client = Arc::clone(&client);
                let album_context = album_context.clone();
                let rate_limit_state = Arc::clone(&rate_limit_state);

                async move {
                    let mut attempt = 0;
                    let max_attempts = 3;

                    loop {
                        // Wait if rate limited BEFORE creating progress bar
                        rate_limit_state.wait_if_rate_limited().await;

                        let pb = downloader.state.multi_progress.as_ref().unwrap().insert_before(downloader.state.status_bar.as_ref().unwrap(),ProgressBar::new_spinner());
                        pb.enable_steady_tick(Duration::from_millis(100));
                        pb.set_style(
                            ProgressStyle::default_spinner()
                                .template("{spinner} [{elapsed_precise}] {msg}")
                                .unwrap(),
                        );
                        pb.set_message(track.title.to_string());

                        let track_id = track.id.to_string();
                        let result = {
                            let client_guard = client.lock().await;
                            client_guard
                                .get_track_postpaywall_playback_info(track_id, Some(TrackPlaybackInfoConfig {
                                    audio_quality: Some(downloader.config.audio_quality.clone()),
                                    ..Default::default()
                                }))
                                .await
                        };

                        if let Err(e) = self.check_allow_streaming(&track) {
                            pb.finish_with_message(format!(
                                "✗ {} (attempt {}/{}, streaming not allowed, skipping...)",
                                track.title,
                                attempt + 1,
                                max_attempts
                            ));
                            return (
                                track.title,
                                Err(e).context("Streaming not allowed for this track"),
                            );
                        }

                        match result {
                            Ok(playback_info) => {
                                rate_limit_state.on_success().await;

                                let result = downloader
                                    .download_track_with_info_pb(DownloadTrackRequest {
                                        track: &track,
                                        playback_info: &playback_info,
                                        output_path: output_dir,
                                        album_context: album_context.clone(),
                                        index: Some(index),
                                        pb: Some(&pb),
                                        media_type,
                                    })
                                    .await;

                                if result.is_ok() {
                                    downloader.update_finished(1).await;
                                    pb.finish();
                                } else {
                                    pb.set_message(format!(
                                        "✗ {} (attempt {}/{}, retrying...)",
                                        track.title,
                                        attempt + 1,
                                        max_attempts
                                    ));
                                    if attempt+1 < max_attempts {
                                        attempt += 1;
                                        // notify rate limit state of error
                                        rate_limit_state.on_error().await;

                                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

                                        continue;
                                    } else {
                                        // notify rate limit state of error
                                        rate_limit_state.on_error().await;

                                        return (
                                            track.title,
                                            Err(TidalError::Other("Unknown error/Timeout".to_string())).context("Unknown error/timeout after multiple attempts"),
                                        );
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

                                if attempt+1 < max_attempts {
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
                }
            })
            .buffer_unordered(self.config.max_parallel)
            .collect::<Vec<_>>()
            .await;

        Ok(DownloadSummary::from_results(results))
    }

    async fn update_finished(&self, amount: usize) {
        self.state
            .finished
            .fetch_add(amount, std::sync::atomic::Ordering::SeqCst);
        let finished = self
            .state
            .finished
            .load(std::sync::atomic::Ordering::SeqCst);
        self.state.status_bar.as_ref().unwrap().set_message(format!(
            "downloading status: {}/{}",
            finished,
            self.state.queued.len()
        ));
    }
}
