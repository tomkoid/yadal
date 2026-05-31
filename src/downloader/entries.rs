use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use tidlers::TidalClient;

use crate::downloader::{Downloader, context::AlbumTagContext, ui::summary::DownloadSummary};

impl Downloader {
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

        let album_context = match client.get_album(track.album.id.to_string()).await {
            Ok(album) => Some(AlbumTagContext::from_album_response(&album)),
            Err(err) => {
                eprintln!(
                    "warning: failed to fetch album metadata for {}: {}",
                    track.album.title, err
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
                Some(&pb),
            )
            .await?;

        if was_downloaded {
            pb.finish_with_message("✓ Downloaded");
        } else {
            pb.finish_with_message("○ Already exists");
        }

        Ok(())
    }

    pub async fn download_album(
        &self,
        client: &mut TidalClient,
        album_id: &str,
        force_recheck: bool,
    ) -> Result<()> {
        let album = client
            .get_album(album_id.to_string())
            .await
            .context("Failed to get album info")?;

        println!("album: {}", album.title);
        println!("artist: {}", album.artist.name);
        println!("tracks: {}", album.number_of_tracks);

        let album_tag_context = AlbumTagContext::from_album_response(&album);

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

        let mut already_downloaded = 0usize;
        let tracks_to_download = if force_recheck {
            all_tracks
        } else {
            let mut filtered = Vec::new();
            for track in all_tracks {
                if self.track_exists_in_directory(&album_dir, track.track_number, &track.title) {
                    already_downloaded += 1;
                } else {
                    filtered.push(track);
                }
            }
            filtered
        };

        if already_downloaded > 0 {
            println!(
                "skipping {} tracks already in directory (use --force-recheck to revalidate)\n",
                already_downloaded
            );
        }

        if tracks_to_download.is_empty() {
            let summary = DownloadSummary {
                downloaded: 0,
                skipped: already_downloaded,
                failed: Vec::new(),
            };
            summary.print();
            return Ok(());
        }

        let mut summary = self
            .download_tracks_parallel(
                client,
                tracks_to_download,
                &album_dir,
                false, // use original track numbers
                Some(album_tag_context),
            )
            .await?;
        summary.skipped += already_downloaded;
        summary.print();
        Ok(())
    }
    pub async fn download_playlist(
        &self,
        client: &mut TidalClient,
        playlist_id: &str,
        force_recheck: bool,
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
                .get_playlist_items(
                    playlist_id.to_string(),
                    Some(limit),
                    Some(offset),
                    None,
                    None,
                )
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

        let mut already_downloaded = 0usize;
        let tracks_to_download = if force_recheck {
            all_tracks
        } else {
            all_tracks
                .into_iter()
                .enumerate()
                .filter_map(|(index, track)| {
                    let track_number = (index + 1) as u32;
                    if self.track_exists_in_directory(&playlist_dir, track_number, &track.title) {
                        already_downloaded += 1;
                        None
                    } else {
                        Some(track)
                    }
                })
                .collect::<Vec<_>>()
        };

        if already_downloaded > 0 {
            println!(
                "skipping {} tracks already in directory (use --force-recheck to revalidate)\n",
                already_downloaded
            );
        }

        if tracks_to_download.is_empty() {
            let summary = DownloadSummary {
                downloaded: 0,
                skipped: already_downloaded,
                failed: Vec::new(),
            };
            summary.print();
            return Ok(());
        }

        let mut summary = self
            .download_tracks_parallel(
                client,
                tracks_to_download,
                &playlist_dir,
                true, // use playlist position as track number
                None,
            )
            .await?;
        summary.skipped += already_downloaded;
        summary.print();
        Ok(())
    }
}
