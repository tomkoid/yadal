use anyhow::{Context, Result};
use multitag::Tag;
use multitag::data::Timestamp;
use std::fs::OpenOptions;
use std::io::Seek;
use std::path::Path;
use std::str::FromStr;

use crate::downloader::Downloader;
use crate::downloader::context::{TagLyrics, TrackTagMetadata};

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

        // title
        if metadata.tag_title {
            let title = if let Some(ref version) = metadata.track_version {
                metadata.title.clone() + &format!(" ({version})")
            } else {
                metadata.title.clone()
            };

            tag.set_title(&title);
        }

        // track number
        if let Some(track_number) = metadata.track_number {
            match tag.set_track_number(track_number) {
                Ok(_) => {}
                Err(e) => {
                    eprintln!(
                        "warning: failed to set track number for {}: {}",
                        metadata.title, e
                    );
                }
            }
        }

        // artists
        if let Some(ref artists) = metadata.artists {
            if artists.len() == 1 {
                tag.set_artist(&artists[0]);
            } else if !artists.is_empty() {
                tag.set_artists(artists.clone());
            }
        }

        // cover
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

        // album info
        // multitag doesn't support setting cover, album title, or album artist separately
        // this implementation could cause empty tags to be written in some cases
        // tidlers only supports getting the first album artist
        // temporary workaround to just set the first one
        let album_artist = metadata.album_artists.as_ref().map(|album_artist| album_artist[0].clone());
        let has_album_info =
            metadata.album_title.is_some() || metadata.album_artists.is_some() || cover.is_some();
        if has_album_info
            && let Err(e) = tag.set_album_info(multitag::data::Album {
                title: metadata.album_title.clone(),
                artist: album_artist,
                cover,
            })
        {
            return Err(anyhow::anyhow!(
                "Failed to set album info for {}: {}",
                metadata.title,
                e
            ));
        }

        // release date
        if let Some(ref date) = metadata.release_date {
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

        // lyrics
        // multitag doesn't support setting UNSYNCEDLYRICS instead of regular LYRICS (synced by convention)
        // this implementation can currently set LYRICS to synced or unsynced lyrics, which is not the best behaviour
        // as well as if both types of lyrics are available, only synced will be set
        match &metadata.lyrics {
            TagLyrics::UnsyncedOnly(unsynced) => tag.set_lyrics(unsynced),
            TagLyrics::SyncedOnly(synced) => tag.set_lyrics(synced),
            TagLyrics::UnsyncedAndSynced(_unsynced, synced) => tag.set_lyrics(synced),
            TagLyrics::None => {},
        }

        // bpm
        if let Some(bpm) = metadata.bpm {
            tag.set_bpm(bpm.round() as u16).unwrap_or_else(|e| {
                eprintln!("warning: failed to set BPM for {}: {}", metadata.title, e);
            });
        }

        // key
        if let Some(key) = metadata.key.as_deref()
            && let Some(key_scale) = metadata.key_scale.as_deref()
        {
            let key_scale = match key_scale.to_lowercase().as_str() {
                "major" => "", // no need to append anything for major keys
                "minor" => "m",
                _ => {
                    eprintln!(
                        "warning: unrecognized key scale '{}' for {}. Using original value.",
                        key_scale, metadata.title
                    );
                    key_scale
                }
            };
            let musical_key = format!("{}{}", key, key_scale.to_lowercase());
            tag.set_key(&musical_key);
        }

        // cannot be set with multitag:
        // copyright
        // disc_number
        // isrc
        // replaygain
        // total_discs
        // total_tracks
        // url

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
