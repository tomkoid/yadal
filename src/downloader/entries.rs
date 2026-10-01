use std::{io, path::Path, sync::Arc, time::Duration};

use anyhow::{Context, Result};
use futures::{StreamExt, stream};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use tidlers::{
    TidalError,
    client::models::track::{Track, config::TrackPlaybackInfoConfig},
};

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

const TRACK_PAGE_LIMIT: u64 = 100;

impl Downloader {
    pub async fn download_track(&self, track_id: &str) -> Result<DownloadSummary> {
        let track = self
            .tidal_client
            .get_track(track_id.to_string())
            .await
            .context("Failed to get track info")?;

        println!("track: {}", track.title);
        println!("artist: {}", track.artist.name);
        println!("album: {}", track.album.as_ref().unwrap().title);

        self.check_allow_streaming(&track)?;

        let playback_info = self
            .tidal_client
            .get_track_postpaywall_playback_info(
                track_id.to_string(),
                Some(TrackPlaybackInfoConfig {
                    audio_quality: Some(self.config.download.audio_quality.clone()),
                    ..Default::default()
                }),
            )
            .await
            .context("Failed to get playback info")?;

        let album_context = match self
            .tidal_client
            .get_album(track.album.as_ref().unwrap().id.to_string())
            .await
        {
            Ok(album) => Some(AlbumTagContext::from_album_response(&album)),
            Err(err) => {
                eprintln!(
                    "warning: failed to fetch album metadata for {}: {}",
                    track.album.as_ref().unwrap().title,
                    err
                );
                None
            }
        };

        let pb = self.create_spinner("Downloading...");

        self.download_track_with_info_pb(DownloadTrackRequest {
            track: &track,
            playback_info: &playback_info,
            output_path: &self.config.download.output_path,
            album_context,
            index: None,
            pb: Some(&pb),
            media_type: MediaType::Track,
        })
        .await?; // temporary while downloading code is being refactored
        // an already existing track file will cause an exiting error right now

        let mut summary = DownloadSummary::new();
        summary.downloaded += 1;

        Ok(summary)
    }

    pub async fn download_media(
        &mut self,
        id: &str,
        media_type: MediaType,
    ) -> Result<DownloadSummary> {
        let (dir_name, album_tag_context) = self
            .resolve_media_dir_and_album_context(id, media_type)
            .await?;

        let target_dir = self
            .config
            .download
            .output_path
            .join(sanitize_filename::sanitize(dir_name));
        std::fs::create_dir_all(&target_dir).context("Failed to create media directory")?;

        let all_tracks = self.fetch_media_tracks(id, media_type).await?;
        let (tracks_to_download, already_downloaded) =
            self.build_download_queue(all_tracks, &target_dir, media_type);

        if already_downloaded > 0 {
            println!(
                "skipping {} tracks already in directory (use --force to redownload)",
                already_downloaded
            );
        }

        if tracks_to_download.is_empty() {
            let summary = DownloadSummary {
                downloaded: 0,
                skipped: already_downloaded,
                failed: Vec::new(),
            };

            return Ok(summary);
        }

        // put tracks into downloader queue
        self.state.queued = tracks_to_download.clone();

        // prepare ui
        let multi_progress = MultiProgress::new();

        let status_bar = multi_progress.add(ProgressBar::hidden());
        status_bar.set_style(ProgressStyle::default_bar().template("{msg}").unwrap());
        status_bar.enable_steady_tick(Duration::from_millis(100));

        self.state.multi_progress = Some(multi_progress);
        self.state.status_bar = Some(status_bar);

        let mut summary = self
            .download_tracks_parallel(&target_dir, album_tag_context, media_type)
            .await?;

        self.state.status_bar.as_ref().unwrap().finish_and_clear();

        summary.skipped += already_downloaded;

        Ok(summary)
    }

    async fn resolve_media_dir_and_album_context(
        &self,
        id: &str,
        media_type: MediaType,
    ) -> Result<(String, Option<AlbumTagContext>)> {
        match media_type {
            MediaType::Album => {
                let album = self
                    .tidal_client
                    .get_album(id.to_string())
                    .await
                    .context("Failed to get album info")?;

                println!("album: {}", album.title);
                println!("artist: {}", album.artist.name);
                println!("tracks: {}", album.number_of_tracks);

                let tag_ctx = AlbumTagContext::from_album_response(&album);
                let dir_name = format!("{} - {}", album.artist.name, album.title);
                Ok((dir_name, Some(tag_ctx)))
            }
            MediaType::Playlist => {
                let playlist = self
                    .tidal_client
                    .get_playlist(id.to_string())
                    .await
                    .context("Failed to get playlist info")?;

                println!("playlist: {}", playlist.title);
                println!("creator: {}", playlist.creator.id);
                println!("tracks: {}", playlist.number_of_tracks);

                Ok((format!("{}-playlist", playlist.title), None))
            }
            _ => panic!("download_media should only be called for albums or playlists"),
        }
    }

    async fn fetch_media_tracks(&self, id: &str, media_type: MediaType) -> Result<Vec<Track>> {
        let mut all_tracks = Vec::new();
        let mut offset = 0;

        loop {
            let total_items: usize = match media_type {
                MediaType::Album => {
                    let items = self
                        .tidal_client
                        .get_album_items(id.to_string(), Some(TRACK_PAGE_LIMIT), Some(offset))
                        .await
                        .context("Failed to get album tracks")?;

                    all_tracks.extend(items.items.into_iter().map(|item| item.item));
                    items.total_number_of_items as usize
                }
                MediaType::Playlist => {
                    let items = self
                        .tidal_client
                        .get_playlist_items(
                            id.to_string(),
                            Some(TRACK_PAGE_LIMIT),
                            Some(offset),
                            None,
                            None,
                        )
                        .await
                        .context("Failed to get playlist tracks")?;

                    all_tracks.extend(items.items.into_iter().map(|item| item.item));
                    items.total_number_of_items as usize
                }
                _ => unreachable!(),
            };

            if all_tracks.len() >= total_items {
                break;
            }

            offset += TRACK_PAGE_LIMIT;
        }

        Ok(all_tracks)
    }

    fn build_download_queue(
        &self,
        tracks: Vec<Track>,
        target_dir: &Path,
        media_type: MediaType,
    ) -> (Vec<QueuedTrack>, usize) {
        let mut already_downloaded = 0usize;
        let mut queued_tracks = Vec::new();

        for (index, track) in tracks.into_iter().enumerate() {
            // handle range filtering if specified
            if let Some(range) = &self.config.download.range
                && !range.contains(&(index + 1))
            {
                continue;
            }

            /*
            if !self.config.download.force_download
                && Self::find_existing_track_path(target_dir, &track, &media_type, Some(index))
                    .is_some()
            {
                already_downloaded += 1;
                continue;
            }
            */

            queued_tracks.push(QueuedTrack { track, index });
        }

        (queued_tracks, already_downloaded)
    }

    pub async fn download_tracks_parallel(
        &self,
        output_dir: &Path,
        album_context: Option<AlbumTagContext>,
        media_type: MediaType,
    ) -> Result<DownloadSummary> {
        println!(
            "\ndownloading {} tracks in parallel (max {})...",
            self.state.queued.len(),
            self.config.download.max_parallel
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
                                    audio_quality: Some(downloader.config.download.audio_quality.clone()),
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
            .buffer_unordered(self.config.download.max_parallel)
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
