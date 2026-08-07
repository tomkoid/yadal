use anyhow::{Context, Result};
use multitag::Tag;
use multitag::data::Timestamp;
use std::fs::OpenOptions;
use std::io::Seek;
use std::path::Path;
use std::str::FromStr;

use crate::downloader::Downloader;
use crate::downloader::context::TrackTagMetadata;

impl Downloader {
    pub async fn tag_downloaded_file(
        &self,
        output_path: &Path,
        metadata: &TrackTagMetadata,
    ) -> Result<()> {
        let extension = output_path
            .extension()
            .and_then(|ext| ext.to_str())
            .context("Failed to determine file extension for tagging")?;

        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(output_path)
            .with_context(|| format!("Failed to open {} for tagging", output_path.display()))?;

        let tag_extension = self.sniff_tag_extension(&mut file, extension)?;
        if tag_extension != extension {
            eprintln!(
                "warning: {} appears to be {} but has .{} extension",
                output_path.display(),
                tag_extension,
                extension
            );
        }

        let tag = Tag::read_from(&tag_extension, &file);

        let mut tag = match tag {
            Ok(tag) => tag,
            Err(_) => {
                return Err(anyhow::anyhow!(
                    "Unsupported file format for tagging or error reading file: {} (detected as {})",
                    output_path.display(),
                    tag_extension
                ));
            }
        };

        tag.set_title(&metadata.title);
        match tag.set_track_number(metadata.track_number) {
            Ok(_) => {}
            Err(e) => {
                eprintln!(
                    "warning: failed to set track number for {}: {}",
                    metadata.title, e
                );
            }
        }

        if metadata.artists.len() == 1 {
            tag.set_artist(&metadata.artists[0]);
        } else if !metadata.artists.is_empty() {
            tag.set_artists(metadata.artists.clone());
        }

        let cover = match metadata.cover_url.as_deref() {
            Some(url) => match self.fetch_cover_picture(url).await {
                Ok(picture) => Some(picture),
                Err(err) => {
                    eprintln!(
                        "warning: failed to download cover art for {}: {}",
                        metadata.title, err
                    );
                    None
                }
            },
            None => None,
        };

        let has_album_info =
            metadata.album_title.is_some() || metadata.album_artist.is_some() || cover.is_some();
        if has_album_info
            && let Err(e) = tag.set_album_info(multitag::data::Album {
                title: metadata.album_title.clone(),
                artist: metadata.album_artist.clone(),
                cover,
            }) {
                return Err(anyhow::anyhow!(
                    "Failed to set album info for {}: {}",
                    metadata.title,
                    e
                ));
            }

        if let Some(date) = metadata.release_date.as_deref() {
            match Timestamp::from_str(date) {
                Ok(timestamp) => tag.set_date(timestamp),
                Err(err) => {
                    eprintln!(
                        "warning: invalid release date '{}' for {}: {}",
                        date, metadata.title, err
                    );
                }
            }
        }

        file.rewind()
            .context("Failed to rewind file before writing tags")?;

        if let Err(e) = tag.write_to_file(&mut file) {
            return Err(anyhow::anyhow!(
                "Failed to write tags to {}: {}",
                output_path.display(),
                e
            ));
        }

        Ok(())
    }
}
