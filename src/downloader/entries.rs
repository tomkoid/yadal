use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use tidlers::client::models::track::config::TrackPlaybackInfoConfig;

use crate::{
    downloader::{Downloader, context::AlbumTagContext, ui::summary::DownloadSummary},
    types::MediaType,
};

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
        {
            println!("skipping track (already exists in output directory, overwrite with --force)");
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

        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} [{elapsed_precise}] {msg}")
                .unwrap(),
        );
        pb.set_message("Downloading...");

        let was_downloaded = self
            .download_track_with_info_pb(
                &track,
                &playback_info,
                &self.output_dir,
                album_context,
                None,
                Some(&pb),
                MediaType::Track,
            )
            .await?;

        let mut summary = DownloadSummary::new();
        if was_downloaded {
            summary.downloaded += 1;
            pb.finish_with_message("✓ Downloaded");
        } else {
            summary.skipped += 1;
            pb.finish_with_message("○ Already exists");
        }

        Ok(summary)
    }

    pub async fn download_media(&self, id: &str, media_type: MediaType) -> Result<DownloadSummary> {
        // fetch metadata
        let (dir_name, album_tag_context) = match media_type {
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
                (dir_name, Some(tag_ctx))
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

                let dir_name = format!("{}-playlist", playlist.title);
                (dir_name, None)
            }
            _ => {
                panic!("download_media should only be called for albums or playlists");
            }
        };

        // create the target directory
        let target_dir = self.output_dir.join(sanitize_filename::sanitize(dir_name));
        std::fs::create_dir_all(&target_dir).context("Failed to create media directory")?;

        // fetch all tracks (handles pagination)
        let mut all_tracks = Vec::new();
        let mut offset = 0;
        let limit = 100;

        loop {
            let total_items: usize;

            match media_type {
                MediaType::Album => {
                    let items = self
                        .tidal_client
                        .get_album_items(id.to_string(), Some(limit), Some(offset))
                        .await
                        .context("Failed to get album tracks")?;

                    for item in items.items {
                        all_tracks.push(item.item);
                    }
                    total_items = items.total_number_of_items as usize;
                }
                MediaType::Playlist => {
                    let items = self
                        .tidal_client
                        .get_playlist_items(id.to_string(), Some(limit), Some(offset), None, None)
                        .await
                        .context("Failed to get playlist tracks")?;

                    for item in items.items {
                        all_tracks.push(item.item);
                    }
                    total_items = items.total_number_of_items as usize;
                }
                _ => unreachable!(),
            }

            if all_tracks.len() >= total_items {
                break;
            }
            offset += limit;
        }

        let mut already_downloaded = 0usize;
        let tracks_to_download = match self.force_download {
            true => all_tracks.clone(),
            false => all_tracks
                .into_iter()
                .enumerate()
                .filter_map(|(index, track)| {
                    if self
                        .find_existing_track_path(&target_dir, &track, &media_type, Some(index))
                        .is_some()
                    {
                        already_downloaded += 1;
                        None
                    } else {
                        Some(track)
                    }
                })
                .collect::<Vec<_>>(),
        };

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

        let use_playlist_position = matches!(media_type, MediaType::Playlist);

        let mut summary = self
            .download_tracks_parallel(
                tracks_to_download,
                &target_dir,
                use_playlist_position,
                album_tag_context,
                media_type,
            )
            .await?;

        summary.skipped += already_downloaded;

        Ok(summary)
    }
}
