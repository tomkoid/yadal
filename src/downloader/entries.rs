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
use tidlers::{
    TidalError,
    client::models::{
        album::AlbumResponse,
        track::{Track, config::TrackPlaybackInfoConfig},
    },
};

use crate::{
    downloader::{
        Downloader,
        context::{AlbumTagContext, PlaylistTemplateContext},
        download::{TrackJob, TrackOutcome},
        rate_limiter::RateLimitState,
        template::Templater,
        ui::summary::DownloadSummary,
        utils::parse_ymdhms,
    },
    parser::Target,
    tracing,
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

struct ResolvedCollection {
    jobs: Vec<TrackJob>,
    failed: Vec<(String, anyhow::Error)>,
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
            let media_type = match forced_type.or(target.media_type) {
                Some(media_type) => media_type,
                None => self.is_album_or_track(&target.id).await?,
            };

            match media_type {
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
                        Ok(collection) => {
                            summary.failed.extend(collection.failed);
                            summary.merge(self.run_batch(collection.jobs, &templater).await);
                        }
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

    async fn get_album_cached(&self, id: &str) -> Result<Arc<AlbumResponse>> {
        if let Some(album) = self.album_cache.lock().await.get(id) {
            return Ok(Arc::clone(album));
        }

        let album = Arc::new(
            self.tidal_client
                .get_album(id.to_string())
                .await
                .context("Failed to get album info")?,
        );

        self.album_cache
            .lock()
            .await
            .insert(id.to_string(), Arc::clone(&album));

        Ok(album)
    }

    async fn resolve_track_album(&self, track: &Track) -> Result<AlbumTagContext> {
        let track_album = track.album.as_ref().context("Track has no album info")?;

        let album = self
            .get_album_cached(&track_album.id.to_string())
            .await
            .with_context(|| format!("Failed to get album info for {}", track_album.title))?;

        Ok(AlbumTagContext::from_album_response(&album))
    }

    async fn resolve_track(&self, id: &str) -> Result<TrackJob> {
        let track = self
            .tidal_client
            .get_track(id.to_string())
            .await
            .context("Failed to get track info")?;

        let album = self.resolve_track_album(&track).await?;

        /*
        println!("track: {}", track.title);
        println!("artist: {}", track.artist.name);
        println!("album: {}", album.title);
        */

        Ok(TrackJob {
            track,
            album,
            playlist: None,
            position: 1,
        })
    }

    async fn is_album_or_track(&self, id: &str) -> std::result::Result<MediaType, TidalError> {
        let (track, album) = tokio::join!(
            self.tidal_client.get_track(id),
            self.tidal_client.get_album(id),
        );

        let track_ok = Self::is_found(track)?;
        let album_ok = Self::is_found(album)?;

        // hopefully these two values somehow aren't true at the same time...
        match (track_ok, album_ok) {
            (true, false) => Ok(MediaType::Track),
            (false, true) => Ok(MediaType::Album),
            _ => Err(TidalError::NotFound),
        }
    }

    fn is_found<T>(response: Result<T, TidalError>) -> Result<bool, TidalError> {
        match response {
            Ok(_) => Ok(true),
            Err(err) => match err {
                TidalError::NotFound => Ok(false),
                err => Err(err),
            },
        }
    }

    /// Whether the 1-based position inside a collection is selected by `range`
    fn position_in_range(&self, position: usize) -> bool {
        self.config
            .download
            .range
            .as_ref()
            .is_none_or(|range| range.contains(&position))
    }

    async fn resolve_collection(
        &self,
        id: &str,
        media_type: MediaType,
    ) -> Result<ResolvedCollection> {
        match media_type {
            MediaType::Album => {
                let album = self.get_album_cached(id).await?;

                /*
                println!("album: {}", album.title);
                println!("artist: {}", album.artist.name);
                println!("tracks: {}", album.number_of_tracks);
                */

                let album = AlbumTagContext::from_album_response(&album);
                let tracks = self.fetch_collection_tracks(id, media_type).await?;

                let jobs = tracks
                    .into_iter()
                    .enumerate()
                    .filter(|(index, _)| self.position_in_range(index + 1))
                    .map(|(index, track)| TrackJob {
                        track,
                        album: album.clone(),
                        playlist: None,
                        position: index + 1,
                    })
                    .collect();

                Ok(ResolvedCollection {
                    jobs,
                    failed: Vec::new(),
                })
            }
            MediaType::Playlist => {
                let playlist = self
                    .tidal_client
                    .get_playlist(id.to_string())
                    .await
                    .context("Failed to get playlist info")?;

                /*
                println!("playlist: {}", playlist.title);
                println!("creator: {}", playlist.creator.id);
                println!("tracks: {}", playlist.number_of_tracks);
                */

                let playlist = PlaylistTemplateContext {
                    uuid: id.to_string(),
                    title: playlist.title.clone(),
                    created: parse_ymdhms(&playlist.created).with_context(|| {
                        format!(
                            "Failed to parse playlist created date '{}'",
                            playlist.created
                        )
                    })?,
                    last_updated: parse_ymdhms(&playlist.last_updated).with_context(|| {
                        format!(
                            "Failed to parse playlist updated date '{}'",
                            playlist.last_updated
                        )
                    })?,
                };

                let tracks = self.fetch_collection_tracks(id, media_type).await?;

                println!("fetching album info for playlist tracks...");

                let mut jobs = Vec::new();
                let mut failed = Vec::new();

                for (index, track) in tracks.into_iter().enumerate() {
                    let position = index + 1;

                    if !self.position_in_range(position) {
                        continue;
                    }

                    // playlist tracks only carry a partial album, so we shall get the full one!
                    match self.resolve_track_album(&track).await {
                        Ok(album) => jobs.push(TrackJob {
                            track,
                            album,
                            playlist: Some(playlist.clone()),
                            position,
                        }),
                        Err(err) => failed.push((track.title.clone(), err)),
                    }
                }

                Ok(ResolvedCollection { jobs, failed })
            }
            MediaType::Track => unreachable!("tracks are resolved with resolve_track"),
        }
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
                "downloading {} tracks in parallel (max {})...",
                jobs.len(),
                max_parallel
            );
        }

        let multi_progress = tracing::multi_progress().clone();
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
        let title = if let Some(ref version) = job.track.version {
            job.track.title.clone() + &format!(" ({version})")
        } else {
            job.track.title.clone()
        };

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
