use std::path::Path;

use anyhow::{Context, Result};
use tidlers::client::models::track::{Track, config::TrackPlaybackInfoConfig};

use crate::{
    downloader::{
        Downloader,
        context::AlbumTagContext,
        download::{DownloadTrackRequest, QueuedTrack},
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

        if self
            .find_existing_track_path(&self.output_dir, &track, &MediaType::Track, None)
            .is_some()
            && !self.force_download
        {
            eprintln!(
                "skipping track (already exists in output directory, overwrite with --force)"
            );
            return Ok(DownloadSummary {
                downloaded: 0,
                skipped: 1,
                failed: Vec::new(),
            });
        }

        let playback_info = self
            .tidal_client
            .get_track_postpaywall_playback_info(
                track_id.to_string(),
                Some(TrackPlaybackInfoConfig {
                    audio_quality: Some(self.audio_quality.clone()),
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
            output_dir: &self.output_dir,
            album_context,
            index: None,
            pb: Some(&pb),
            media_type: MediaType::Track,
        })
        .await?;

        let mut summary = DownloadSummary::new();
        summary.downloaded += 1;
        pb.finish_with_message("Downloaded");

        Ok(summary)
    }

    pub async fn download_media(&self, id: &str, media_type: MediaType) -> Result<DownloadSummary> {
        let (dir_name, album_tag_context) = self
            .resolve_media_dir_and_album_context(id, media_type)
            .await?;

        let target_dir = self.output_dir.join(sanitize_filename::sanitize(dir_name));
        std::fs::create_dir_all(&target_dir).context("Failed to create media directory")?;

        let all_tracks = self.fetch_media_tracks(id, media_type).await?;
        let (tracks_to_download, already_downloaded) =
            self.build_download_queue(all_tracks, &target_dir, media_type);

        if already_downloaded > 0 {
            println!(
                "skipping {} tracks already in directory (use --force to redownload)\n",
                already_downloaded
            );
        }

        if tracks_to_download.is_empty() {
            let summary = DownloadSummary {
                downloaded: 0,
                skipped: already_downloaded,
                failed: Vec::new(),
            };

            if matches!(media_type, MediaType::Album) {
                summary.print();
            }

            return Ok(summary);
        }

        let mut summary = self
            .download_tracks_parallel(
                tracks_to_download,
                &target_dir,
                album_tag_context,
                media_type,
            )
            .await?;

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
            if !self.force_download
                && self
                    .find_existing_track_path(target_dir, &track, &media_type, Some(index))
                    .is_some()
            {
                already_downloaded += 1;
                continue;
            }

            queued_tracks.push(QueuedTrack { track, index });
        }

        (queued_tracks, already_downloaded)
    }
}
