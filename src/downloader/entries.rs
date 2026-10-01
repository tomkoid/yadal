use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use anyhow::{Context, Result};
use futures::{StreamExt, stream};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use tidlers::client::models::track::{Track, config::TrackPlaybackInfoConfig};

use crate::{
    downloader::{
        Downloader,
        context::{AlbumTagContext, PlaylistTemplateContext},
        download::{TrackJob, TrackOutcome},
        rate_limiter::RateLimitState,
        template::Templater,
        ui::summary::DownloadSummary,
    },
    parser::Target,
    types::MediaType,
};

const TRACK_PAGE_LIMIT: u64 = 100;
const MAX_ATTEMPTS: usize = 3;

struct Batch<'a> {
    multi_progress: MultiProgress,
    status_bar: ProgressBar,
    finished: AtomicUsize,
    total: usize,
    rate_limit: Arc<RateLimitState>,
    templater: &'a Templater,
}

impl Batch<'_> {
    fn update_finished(&self, amount: usize) {
        let finished = self.finished.fetch_add(amount, Ordering::SeqCst) + amount;

        self.status_bar
            .set_message(format!("downloading status: {}/{}", finished, self.total));
    }
}

impl Downloader {
    /// download a list of track(s), album(s), or playlist(s)
    pub async fn download_media(
        &self,
        targets: &[Target],
        forced_type: Option<MediaType>,
    ) -> Result<DownloadSummary> {
        let templater = Templater::make(&self.config.download.output_template)
            .context("Invalid output template")?;

        let mut summary = DownloadSummary::new();
        let mut pending_tracks: Vec<TrackJob> = Vec::new();

        for target in targets {
            match forced_type.unwrap_or(target.media_type) {
                MediaType::Track => match self.resolve_track(&target.id).await {
                    Ok(job) => pending_tracks.push(job),
                    Err(err) => summary.failed.push((target.id.clone(), err)),
                },
                media_type => {
                    if !pending_tracks.is_empty() {
                        let jobs = std::mem::take(&mut pending_tracks);
                        summary.merge(self.run_batch(jobs, &templater).await);
                    }

                    match self.resolve_collection(&target.id, media_type).await {
                        Ok(jobs) => summary.merge(self.run_batch(jobs, &templater).await),
                        Err(err) => summary.failed.push((target.id.clone(), err)),
                    }
                }
            }
        }

        if !pending_tracks.is_empty() {
            summary.merge(self.run_batch(pending_tracks, &templater).await);
        }

        Ok(summary)
    }

    async fn resolve_track(&self, id: &str) -> Result<TrackJob> {
        let track = self
            .tidal_client
            .get_track(id.to_string())
            .await
            .context("Failed to get track info")?;

        println!("track: {}", track.title);
        println!("artist: {}", track.artist.name);

        let album = match track.album.as_ref() {
            Some(track_album) => {
                println!("album: {}", track_album.title);
                match self
                    .tidal_client
                    .get_album(track_album.id.to_string())
                    .await
                {
                    Ok(album) => Some(AlbumTagContext::from_album_response(&album)),
                    Err(err) => {
                        eprintln!(
                            "warning: failed to fetch album metadata for {}: {}",
                            track_album.title, err
                        );
                        None
                    }
                }
            }
            None => None,
        };

        Ok(TrackJob {
            track,
            album,
            playlist: None,
            position: 1,
        })
    }

    async fn resolve_collection(&self, id: &str, media_type: MediaType) -> Result<Vec<TrackJob>> {
        let (album, playlist) = match media_type {
            MediaType::Album => {
                let album = self
                    .tidal_client
                    .get_album(id.to_string())
                    .await
                    .context("Failed to get album info")?;

                println!("album: {}", album.title);
                println!("artist: {}", album.artist.name);
                println!("tracks: {}", album.number_of_tracks);

                (Some(AlbumTagContext::from_album_response(&album)), None)
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

                (
                    None,
                    Some(PlaylistTemplateContext {
                        uuid: id.to_string(),
                        title: playlist.title.clone(),
                        created: Some(playlist.created.clone()),
                        last_updated: Some(playlist.last_updated.clone()),
                    }),
                )
            }
            MediaType::Track => unreachable!("tracks are resolved with resolve_track"),
        };

        let tracks = self.fetch_collection_tracks(id, media_type).await?;

        Ok(tracks
            .into_iter()
            .enumerate()
            .filter(|(index, _)| {
                self.config
                    .download
                    .range
                    .as_ref()
                    .is_none_or(|range| range.contains(&(index + 1)))
            })
            .map(|(index, track)| TrackJob {
                track,
                album: album.clone(),
                playlist: playlist.clone(),
                position: index + 1,
            })
            .collect())
    }

    async fn fetch_collection_tracks(&self, id: &str, media_type: MediaType) -> Result<Vec<Track>> {
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

    async fn run_batch(&self, jobs: Vec<TrackJob>, templater: &Templater) -> DownloadSummary {
        let mut summary = DownloadSummary::new();
        if jobs.is_empty() {
            return summary;
        }

        let max_parallel = self.config.download.max_parallel.max(1);

        if jobs.len() > 1 {
            println!(
                "\ndownloading {} tracks in parallel (max {})...",
                jobs.len(),
                max_parallel
            );
        }

        let multi_progress = MultiProgress::new();
        let status_bar = multi_progress.add(ProgressBar::hidden());
        status_bar.set_style(ProgressStyle::default_bar().template("{msg}").unwrap());
        status_bar.enable_steady_tick(Duration::from_millis(100));

        let rate_limit = RateLimitState::new();
        rate_limit.set_multi_progress(multi_progress.clone()).await;

        let batch = Batch {
            multi_progress,
            status_bar,
            finished: AtomicUsize::new(0),
            total: jobs.len(),
            rate_limit,
            templater,
        };
        batch.update_finished(0);

        let results = stream::iter(jobs.iter())
            .map(|job| self.run_job(job, &batch))
            .buffer_unordered(max_parallel)
            .collect::<Vec<_>>()
            .await;

        batch.status_bar.finish_and_clear();

        for (title, result) in results {
            match result {
                Ok(TrackOutcome::Downloaded) => summary.downloaded += 1,
                Ok(TrackOutcome::Skipped) => summary.skipped += 1,
                Err(err) => summary.failed.push((title, err)),
            }
        }

        summary
    }

    /// process one track (job)
    async fn run_job(&self, job: &TrackJob, batch: &Batch<'_>) -> (String, Result<TrackOutcome>) {
        let title = job.track.title.clone();

        if let Err(err) = self.check_allow_streaming(&job.track) {
            return (
                title,
                Err(err.context("Streaming not allowed for this track")),
            );
        }

        let mut attempt = 0;

        loop {
            batch.rate_limit.wait_if_rate_limited().await;

            let pb = batch
                .multi_progress
                .insert_before(&batch.status_bar, ProgressBar::new_spinner());
            pb.enable_steady_tick(Duration::from_millis(100));
            pb.set_style(
                ProgressStyle::default_spinner()
                    .template("{spinner} [{elapsed_precise}] {msg}")
                    .unwrap(),
            );
            pb.set_message(title.clone());

            let playback_info = self
                .tidal_client
                .get_track_postpaywall_playback_info(
                    job.track.id.to_string(),
                    Some(TrackPlaybackInfoConfig {
                        audio_quality: Some(self.config.download.audio_quality.clone()),
                        ..Default::default()
                    }),
                )
                .await;

            let outcome = match playback_info {
                Ok(info) => {
                    batch.rate_limit.on_success().await;
                    self.process_track(job, &info, batch.templater, Some(&pb))
                        .await
                }
                Err(err) => Err(anyhow::Error::from(err).context("Failed to get playback info")),
            };

            match outcome {
                Ok(outcome) => {
                    batch.update_finished(1);
                    if matches!(outcome, TrackOutcome::Downloaded) {
                        pb.finish();
                    }
                    return (title, Ok(outcome));
                }
                Err(err) => {
                    batch.rate_limit.on_error().await;

                    if attempt + 1 < MAX_ATTEMPTS {
                        pb.finish_with_message(format!(
                            "✗ {} (attempt {}/{}, retrying...)",
                            title,
                            attempt + 1,
                            MAX_ATTEMPTS
                        ));

                        attempt += 1;

                        tokio::time::sleep(Duration::from_secs(2)).await;
                    } else {
                        pb.finish_with_message(format!("✗ {} (failed)", title));
                        return (title, Err(err));
                    }
                }
            }
        }
    }
}
