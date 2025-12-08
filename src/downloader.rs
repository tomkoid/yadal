use anyhow::{Context, Result};
use bytes::Bytes;
use futures::stream::{self, StreamExt};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tidlers::client::{
    TidalClient,
    models::track::{ManifestType, Track, TrackPlaybackInfoPostPaywallResponse},
};

/// Struct for handling all download operations
pub struct Downloader {
    output_dir: PathBuf,
    http_client: reqwest::Client,
    max_parallel: usize,
}

struct DownloadSummary {
    downloaded: usize,
    skipped: usize,
    failed: Vec<(String, anyhow::Error)>,
}

// Rate limiting state shared across all downloads
struct RateLimitState {
    is_rate_limited: AtomicBool,
    consecutive_errors: AtomicU64,
    last_backoff_time: Arc<tokio::sync::Mutex<Option<std::time::Instant>>>,
    rate_limit_lock: Arc<tokio::sync::Mutex<()>>,
    multi_progress: Arc<tokio::sync::Mutex<Option<MultiProgress>>>,
}

impl RateLimitState {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            is_rate_limited: AtomicBool::new(false),
            consecutive_errors: AtomicU64::new(0),
            last_backoff_time: Arc::new(tokio::sync::Mutex::new(None)),
            rate_limit_lock: Arc::new(tokio::sync::Mutex::new(())),
            multi_progress: Arc::new(tokio::sync::Mutex::new(None)),
        })
    }

    async fn set_multi_progress(&self, mp: MultiProgress) {
        let mut guard = self.multi_progress.lock().await;
        *guard = Some(mp);
    }

    async fn on_error(&self) {
        let errors = self.consecutive_errors.fetch_add(1, Ordering::SeqCst) + 1;

        // If we hit 3 consecutive errors, trigger rate limit backoff
        if errors >= 3 {
            // Use lock to ensure only one thread prints the message
            let _guard = self.rate_limit_lock.lock().await;

            // Check again after acquiring lock
            if !self.is_rate_limited.swap(true, Ordering::SeqCst) {
                // Suspend multi-progress to stop all updates
                // if let Some(mp) = self.multi_progress.lock().await.as_ref() {
                //     mp.suspend(|| {
                //         println!("\nrate limit detected! pausing all downloads for 5 seconds...");
                //     });
                // } else {
                //     println!("\nrate limit detected! pausing all downloads for 5 seconds...");
                // }
                let mut last_time = self.last_backoff_time.lock().await;
                *last_time = Some(std::time::Instant::now());
            }
        }
    }

    async fn on_success(&self) {
        // Only reset if not rate limited
        if !self.is_rate_limited.load(Ordering::SeqCst) {
            self.consecutive_errors.store(0, Ordering::SeqCst);
        }
    }

    async fn wait_if_rate_limited(&self) {
        if self.is_rate_limited.load(Ordering::SeqCst) {
            // Use lock to ensure only one thread does the wait and reset
            let _guard = self.rate_limit_lock.lock().await;

            // Check again after acquiring lock
            if self.is_rate_limited.load(Ordering::SeqCst) {
                let mut last_time = self.last_backoff_time.lock().await;

                if let Some(backoff_start) = *last_time {
                    let elapsed = backoff_start.elapsed();
                    let backoff_duration = std::time::Duration::from_secs(5);

                    if elapsed < backoff_duration {
                        let remaining = backoff_duration - elapsed;
                        drop(last_time); // Release lock before sleeping
                        tokio::time::sleep(remaining).await;
                        last_time = self.last_backoff_time.lock().await;
                    }

                    // Reset rate limit state
                    *last_time = None;
                    self.is_rate_limited.store(false, Ordering::SeqCst);
                    self.consecutive_errors.store(0, Ordering::SeqCst);
                    // if let Some(mp) = self.multi_progress.lock().await.as_ref() {
                    //     mp.suspend(|| {
                    //         println!("resuming downloads...");
                    //     });
                    // } else {
                    //     println!("resuming downloads...");
                    // }
                }
            }
        }
    }
}

impl DownloadSummary {
    fn new() -> Self {
        Self {
            downloaded: 0,
            skipped: 0,
            failed: Vec::new(),
        }
    }

    fn from_results(results: Vec<(String, Result<bool>)>) -> Self {
        let mut summary = Self::new();
        for (track_name, result) in results {
            match result {
                Ok(true) => summary.downloaded += 1,
                Ok(false) => summary.skipped += 1,
                Err(e) => summary.failed.push((track_name, e)),
            }
        }
        summary
    }

    fn print(&self) {
        println!("\nsummary:");
        println!("  downloaded: {}", self.downloaded);
        if self.skipped > 0 {
            println!("  skipped: {} (already exist)", self.skipped);
        }
        if !self.failed.is_empty() {
            println!("  failed: {}", self.failed.len());
            for track in &self.failed {
                println!("    - {} ({})", track.0, track.1.to_string());
            }
        }
    }
}

impl Downloader {
    pub fn new(output_dir: PathBuf, max_parallel: usize) -> Self {
        Self {
            output_dir,
            http_client: reqwest::Client::new(),
            max_parallel,
        }
    }

    pub async fn download_track(&self, client: &mut TidalClient, track_id: &str) -> Result<()> {
        let track = client
            .get_track(track_id.to_string())
            .await
            .context("Failed to get track info")?;

        println!("track: {}", track.title);
        println!("artist: {}", track.artist.name);
        println!("album: {}", track.album.title);

        let playback_info = client
            .get_track_postpaywall_playback_info(track_id.to_string())
            .await
            .context("Failed to get playback info")?;

        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} [{elapsed_precise}] {msg}")
                .unwrap(),
        );
        pb.set_message("Downloading...");

        let was_downloaded = self
            .download_track_with_info_pb(&track, &playback_info, &self.output_dir, Some(&pb))
            .await?;

        if was_downloaded {
            pb.finish_with_message("✓ Downloaded");
        } else {
            pb.finish_with_message("○ Already exists");
        }

        Ok(())
    }

    pub async fn download_album(&self, client: &mut TidalClient, album_id: &str) -> Result<()> {
        let album = client
            .get_album(album_id.to_string())
            .await
            .context("Failed to get album info")?;

        println!("album: {}", album.title);
        println!("artist: {}", album.artist.name);
        println!("tracks: {}", album.number_of_tracks);

        let album_dir = self.output_dir.join(sanitize_filename::sanitize(format!(
            "{} - {}",
            album.artist.name, album.title
        )));
        std::fs::create_dir_all(&album_dir).context("Failed to create album directory")?;

        // fetch all tracks from the album (handles pagination)
        let mut all_tracks = Vec::new();
        let mut offset = 0;
        let limit = 100;

        loop {
            let items = client
                .get_album_items(album_id.to_string(), Some(limit), Some(offset))
                .await
                .context("Failed to get album tracks")?;

            for item in items.items {
                all_tracks.push(item.item);
            }

            if all_tracks.len() >= items.total_number_of_items as usize {
                break;
            }
            offset += limit;
        }

        self.download_tracks_parallel(
            client, all_tracks, &album_dir, false, // use original track numbers
        )
        .await
    }
    pub async fn download_playlist(
        &self,
        client: &mut TidalClient,
        playlist_id: &str,
    ) -> Result<()> {
        let playlist = client
            .get_playlist(playlist_id.to_string())
            .await
            .context("Failed to get playlist info")?;

        println!("playlist: {}", playlist.title);
        println!("creator: {}", playlist.creator.id);
        println!("tracks: {}", playlist.number_of_tracks);

        let playlist_dir = self.output_dir.join(sanitize_filename::sanitize(format!(
            "{}-playlist",
            playlist.title
        )));
        std::fs::create_dir_all(&playlist_dir).context("Failed to create playlist directory")?;

        // fetch all tracks from the playlist (handles pagination)
        let mut all_tracks = Vec::new();
        let mut offset = 0;
        let limit = 100;

        loop {
            let items = client
                .get_playlist_items(playlist_id.to_string(), Some(limit), Some(offset))
                .await
                .context("Failed to get playlist tracks")?;

            for item in items.items {
                all_tracks.push(item.item);
            }

            if all_tracks.len() >= items.total_number_of_items as usize {
                break;
            }
            offset += limit;
        }

        self.download_tracks_parallel(
            client,
            all_tracks,
            &playlist_dir,
            true, // use playlist position as track number
        )
        .await
    }

    async fn download_tracks_parallel(
        &self,
        client: &mut TidalClient,
        tracks: Vec<Track>,
        output_dir: &PathBuf,
        use_index_as_track_number: bool,
    ) -> Result<()> {
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
            .map(async |(index, track)| {
                let downloader = Arc::clone(&downloader);
                let client = Arc::clone(&client);
                let output_dir = output_dir.clone();
                let rate_limit_state = Arc::clone(&rate_limit_state);
                let multi_progress = multi_progress.clone();
                let mut attempt = 0;
                let max_attempts = 10;

                loop {
                    // Wait if rate limited BEFORE creating progress bar
                    rate_limit_state.wait_if_rate_limited().await;

                    let track_number = if use_index_as_track_number {
                        (index + 1) as u32
                    } else {
                        track.track_number
                    };

                    let format_str = if use_index_as_track_number {
                        format!("{:03} - {}", track_number, track.title)
                    } else {
                        format!("{:02} - {}", track_number, track.title)
                    };

                    let pb = multi_progress.add(ProgressBar::new_spinner());
                    pb.set_style(
                        ProgressStyle::default_spinner()
                            .template("{spinner:.green} [{elapsed_precise}] {msg}")
                            .unwrap(),
                    );
                    pb.set_message(format!("{}", format_str));

                    let track_id = track.id.to_string();
                    let result = {
                        let mut client_guard = client.lock().await;
                        client_guard
                            .get_track_postpaywall_playback_info(track_id)
                            .await
                    };

                    match result {
                        Ok(playback_info) => {
                            rate_limit_state.on_success().await;

                            let result = downloader
                                .download_track_with_info_numbered_pb(
                                    &track,
                                    &playback_info,
                                    &output_dir,
                                    track_number,
                                    Some(&pb),
                                )
                                .await;

                            if result.is_ok() {
                                match result.as_ref().unwrap() {
                                    true => pb.finish_with_message(format!("✓ {}", format_str)),
                                    false => pb.finish_with_message(format!("○ {}", format_str)),
                                }
                            } else {
                                pb.finish_with_message(format!(
                                    "✗ {} (attempt {}/{})",
                                    format_str,
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

                            return (format_str, result);
                        }
                        Err(e) => {
                            pb.finish_with_message(format!(
                                "✗ {} (attempt {}/{}, retrying later...)",
                                format_str,
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

                                return (format_str, Err(e).context("Failed to get playback info"));
                            }
                        }
                    }
                }
            })
            .buffer_unordered(self.max_parallel)
            .collect::<Vec<_>>()
            .await;

        DownloadSummary::from_results(results).print();
        Ok(())
    }
    async fn download_track_with_info_pb(
        &self,
        track: &Track,
        playback_info: &TrackPlaybackInfoPostPaywallResponse,
        output_dir: &PathBuf,
        pb: Option<&ProgressBar>,
    ) -> Result<bool> {
        self.download_track_with_info_numbered_pb(
            track,
            playback_info,
            output_dir,
            track.track_number,
            pb,
        )
        .await
    }

    async fn download_track_with_info_numbered_pb(
        &self,
        track: &Track,
        playback_info: &TrackPlaybackInfoPostPaywallResponse,
        output_dir: &PathBuf,
        track_number: u32,
        pb: Option<&ProgressBar>,
    ) -> Result<bool> {
        let extension = self.get_file_extension(playback_info);
        let base_name = format!(
            "{:03} - {}",
            track_number,
            sanitize_filename::sanitize(&track.title)
        );

        // check if file exists with current extension
        let output_path = output_dir.join(format!("{}.{}", base_name, extension));

        if output_path.exists() {
            return Ok(false); // file was skipped
        }

        // check if file exists with different extension (different quality already downloaded)
        let possible_extensions = ["m4a", "flac", "mp3"];
        for ext in &possible_extensions {
            if ext != &extension {
                let other_path = output_dir.join(format!("{}.{}", base_name, ext));
                if other_path.exists() {
                    // delete the old file to replace it with new quality
                    std::fs::remove_file(&other_path)
                        .context("Failed to remove old file with different quality")?;
                }
            }
        }

        match &playback_info.manifest_parsed {
            Some(ManifestType::Dash(dash)) => {
                self.download_dash_track_pb(dash, &output_path, pb).await?;
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

        Ok(true) // file was downloaded
    }

    async fn download_dash_track_pb(
        &self,
        dash: &tidlers::client::models::track::DashManifest,
        output_path: &PathBuf,
        pb: Option<&ProgressBar>,
    ) -> Result<()> {
        let mut combined_data = Vec::new();

        // Step 1: Download initialization segment (required for DASH)
        if let Some(init_url) = dash.get_init_url() {
            if let Some(pb) = pb {
                pb.set_message("Downloading init segment...");
            }
            let init_data = self.download_segment(init_url).await?;
            combined_data.extend_from_slice(&init_data);
        } else {
            anyhow::bail!("No initialization segment found");
        }

        // Step 2: Download media segments sequentially until we hit 3 consecutive failures
        let mut segment_num = 1;
        let mut consecutive_failures = 0;

        loop {
            if let Some(segment_url) = dash.get_segment_url(segment_num) {
                if let Some(pb) = pb {
                    pb.set_message(format!("Downloading segment {}", segment_num));
                }
                match self.download_segment(&segment_url).await {
                    Ok(segment_data) => {
                        combined_data.extend_from_slice(&segment_data);
                        consecutive_failures = 0;
                    }
                    Err(_) => {
                        consecutive_failures += 1;
                        if consecutive_failures >= 3 {
                            break;
                        }
                    }
                }
            } else {
                break;
            }
            segment_num += 1;
        }

        if let Some(pb) = pb {
            pb.set_message("Writing to disk...");
        }

        // Write to file
        std::fs::write(output_path, combined_data).context("Failed to write file")?;

        Ok(())
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

    async fn download_segment(&self, url: &str) -> Result<Bytes> {
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

    fn get_file_extension(&self, playback_info: &TrackPlaybackInfoPostPaywallResponse) -> &str {
        // Determine file extension based on manifest type and MIME type
        match &playback_info.manifest_parsed {
            Some(ManifestType::Dash(_)) => "flac", // HiRes uses fragmented MP4 (m4a)
            Some(ManifestType::Json(json)) => {
                // Standard qualities - check MIME type
                if json.mime_type.contains("flac") {
                    "flac"
                } else if json.mime_type.contains("mp4") || json.mime_type.contains("m4a") {
                    "m4a"
                } else {
                    "m4a"
                }
            }
            None => "m4a",
        }
    }
}
