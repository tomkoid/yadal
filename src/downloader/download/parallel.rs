use std::{path::Path, sync::Arc, time::Duration};

use anyhow::{Context, Result};
use futures::{StreamExt, stream};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
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
        queued_tracks: Vec<QueuedTrack>,
        output_dir: &Path,
        album_context: Option<AlbumTagContext>,
        media_type: MediaType,
    ) -> Result<DownloadSummary> {
        println!(
            "\ndownloading {} tracks in parallel (max {})...",
            queued_tracks.len(),
            self.max_parallel
        );

        let downloader = Arc::new(self);
        let client = Arc::new(tokio::sync::Mutex::new(self.tidal_client.clone()));
        let rate_limit_state = RateLimitState::new();

        // Create multi-progress bar
        let multi_progress = MultiProgress::new();
        rate_limit_state
            .set_multi_progress(multi_progress.clone())
            .await;

        let results = stream::iter(queued_tracks)
            .map(async |queued_track| {
                let QueuedTrack { track, index } = queued_track;
                let downloader = Arc::clone(&downloader);
                let client = Arc::clone(&client);
                let album_context = album_context.clone();
                let rate_limit_state = Arc::clone(&rate_limit_state);
                let multi_progress = multi_progress.clone();
                let mut attempt = 0;
                let max_attempts = 3;

                loop {
                    // Wait if rate limited BEFORE creating progress bar
                    rate_limit_state.wait_if_rate_limited().await;

                    let pb = multi_progress.add(ProgressBar::new_spinner());
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
                                audio_quality: Some(self.audio_quality.clone()),
                                ..Default::default()
                            }))
                            .await
                    };

                    match result {
                        Ok(playback_info) => {
                            rate_limit_state.on_success().await;

                            let result = downloader
                                .download_track_with_info_pb(DownloadTrackRequest {
                                    track: &track,
                                    playback_info: &playback_info,
                                    output_dir,
                                    album_context: album_context.clone(),
                                    index: Some(index),
                                    pb: Some(&pb),
                                    media_type,
                                })
                                .await;

                            if result.is_ok() {
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
            })
            .buffer_unordered(self.max_parallel)
            .collect::<Vec<_>>()
            .await;

        Ok(DownloadSummary::from_results(results))
    }
}
