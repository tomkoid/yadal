use std::{
    io::{Read, Seek},
    path::{Path, PathBuf},
};

use crate::{downloader::Downloader, types::MediaType};

use anyhow::{Context, Result};
use multitag::data::Picture;
use reqwest::header::CONTENT_TYPE;
use tidlers::client::models::track::{
    Track,
    playback::{ManifestType, TrackPlaybackInfoResponse},
};
use tokio::process::Command;

impl Downloader {
    pub async fn maybe_convert_flac_container(
        &self,
        output_path: &Path,
        playback_info: &TrackPlaybackInfoResponse,
    ) -> Result<PathBuf> {
        let extension = output_path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("m4a")
            .to_ascii_lowercase();

        let codecs = playback_info
            .get_codecs()
            .unwrap_or_default()
            .to_ascii_lowercase();

        if extension != "m4a" || !codecs.contains("flac") {
            return Ok(output_path.to_path_buf());
        }

        let flac_path = output_path.with_extension("flac");
        self.transcode_to_flac(output_path, &flac_path).await?;
        std::fs::remove_file(output_path)
            .with_context(|| format!("Failed to remove {}", output_path.display()))?;
        Ok(flac_path)
    }

    async fn transcode_to_flac(&self, input: &Path, output: &Path) -> Result<()> {
        let status = Command::new("ffmpeg")
            .arg("-hide_banner")
            .arg("-loglevel")
            .arg("error")
            .arg("-y")
            .arg("-i")
            .arg(input)
            .arg("-map_metadata")
            .arg("-1")
            .arg("-c:a")
            .arg("flac")
            .arg(output)
            .status()
            .await
            .context("Failed to run ffmpeg for FLAC conversion")?;

        if !status.success() {
            anyhow::bail!(
                "ffmpeg failed converting {} to {}",
                input.display(),
                output.display()
            );
        }

        Ok(())
    }

    pub fn sniff_tag_extension(&self, file: &mut std::fs::File, declared: &str) -> Result<String> {
        let mut header = [0u8; 12];
        let read = file
            .read(&mut header)
            .context("Failed to read file header")?;
        file.rewind()
            .context("Failed to rewind file after header read")?;

        if read >= 8 && &header[4..8] == b"ftyp" {
            return Ok("m4a".to_string());
        }
        if read >= 4 && &header[0..4] == b"fLaC" {
            return Ok("flac".to_string());
        }
        if read >= 4 && &header[0..4] == b"OggS" {
            return Ok("ogg".to_string());
        }
        if read >= 3 && &header[0..3] == b"ID3" {
            return Ok("mp3".to_string());
        }

        Ok(declared.to_string())
    }

    pub fn find_existing_track_path(
        &self,
        output_dir: &Path,
        track: &Track,
        media_type: &MediaType,
        index: Option<usize>, // only used for playlists to determine track number
    ) -> Option<PathBuf> {
        let base_name = self.get_track_base_name(track, media_type, index);

        for ext in ["flac", "m4a", "mp3"] {
            let path = output_dir.join(format!("{}.{}", base_name, ext));
            if path.exists() {
                return Some(path);
            }
        }
        None
    }

    /// get the base name for a track file, including track number and sanitized title
    /// this will be useful in the future if we want to support custom formatting of track file
    /// names
    pub fn get_track_base_name(
        &self,
        track: &Track,
        media_type: &MediaType,
        index: Option<usize>,
    ) -> String {
        if MediaType::Track == *media_type {
            return sanitize_filename::sanitize(&track.title);
        }

        // use album original track numbers and for playlists use their positional index
        let track_number = match media_type {
            MediaType::Album => track.track_number,
            MediaType::Playlist => {
                (index.expect("track is in playlist but no index supplied") + 1) as u32
            }
            _ => unreachable!(),
        };

        format!(
            "{:02} {}",
            track_number,
            sanitize_filename::sanitize(&track.title)
        )
    }

    pub async fn fetch_cover_picture(&self, cover_url: &str) -> Result<Picture> {
        let response = self
            .http_client
            .get(cover_url)
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await
            .context("Failed to request cover art")?;

        if !response.status().is_success() {
            anyhow::bail!("Cover art HTTP {}", response.status());
        }

        let mime_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(|value| value.split(';').next().unwrap_or(value).to_string())
            .unwrap_or_else(|| "image/jpeg".to_string());

        let data = response
            .bytes()
            .await
            .context("Failed to read cover art bytes")?
            .to_vec();

        Ok(Picture { data, mime_type })
    }

    pub fn get_file_extension(&self, playback_info: &TrackPlaybackInfoResponse) -> &str {
        // Determine file extension based on container/MIME type
        if let Some(mime_type) = playback_info.get_mime_type()
            && let Some(ext) = Self::extension_from_mime_type(&mime_type)
        {
            return ext;
        }

        match &playback_info.manifest_parsed {
            Some(ManifestType::Dash(_)) => "m4a", // DASH uses MP4 container
            Some(ManifestType::Json(json)) => {
                if let Some(ext) = Self::extension_from_mime_type(&json.mime_type) {
                    return ext;
                }
                "m4a"
            }
            None => "m4a",
        }
    }

    fn extension_from_mime_type(mime_type: &str) -> Option<&'static str> {
        let mime_type = mime_type.to_ascii_lowercase();
        if mime_type.contains("flac") && !mime_type.contains("mp4") {
            return Some("flac");
        }
        if mime_type.contains("mp4") || mime_type.contains("m4a") {
            return Some("m4a");
        }
        if mime_type.contains("ogg") {
            return Some("ogg");
        }
        if mime_type.contains("mpeg") || mime_type.contains("mp3") {
            return Some("mp3");
        }
        None
    }

    pub fn check_allow_streaming(&self, track: &Track) -> Result<()> {
        if !track.allow_streaming && !self.config.no_stream_check {
            anyhow::bail!("track is not available for streaming (use -f to force download)");
        }

        Ok(())
    }
}
