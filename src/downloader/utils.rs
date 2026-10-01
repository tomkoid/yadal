use std::{
    fs::File,
    io::{BufWriter, Read, Seek, Write},
    path::{Path, PathBuf},
};

use crate::downloader::Downloader;

use anyhow::{Context, Result, bail};
use multitag::data::Picture;
use reqwest::header::CONTENT_TYPE;
use symphonia::core::codecs::CodecParameters;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use tidlers::client::models::track::{
    Track,
    playback::{ManifestType, TrackPlaybackInfoResponse},
};

impl Downloader {
    pub fn needs_flac_remux(playback_info: &TrackPlaybackInfoResponse, extension: &str) -> bool {
        extension.eq_ignore_ascii_case("m4a")
            && playback_info
                .get_codecs()
                .unwrap_or_default()
                .to_ascii_lowercase()
                .contains("flac")
    }

    pub async fn remux_to_flac(input: PathBuf, output: PathBuf) -> Result<()> {
        tokio::task::spawn_blocking(move || remux_mp4_flac_to_flac(&input, &output))
            .await
            .context("remux task panicked")?
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

    pub async fn fetch_cover_picture(&self, cover_url: &str) -> Result<Picture> {
        let response = self
            .http_client
            .get(cover_url)
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await
            .context("Failed to request cover art")?;

        if !response.status().is_success() {
            bail!("Cover art HTTP {}", response.status());
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

    pub fn get_file_extension(playback_info: &TrackPlaybackInfoResponse) -> &str {
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
        if !track.allow_streaming && !self.config.download.no_stream_check {
            bail!("track is not available for streaming (use -f to force download)");
        }

        Ok(())
    }
}

/// Remux MP4 wrapped FLAC frames to a FLAC file directly. \
/// This function does not compute the SEEKTABLE block, which has a very small chance of ever being a problem for anyone ever.
fn remux_mp4_flac_to_flac(input: &Path, output: &Path) -> Result<()> {
    let file = File::open(input).with_context(|| format!("opening {}", input.display()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    hint.with_extension("m4a");

    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .context("error probing mp4 container")?;

    let track = format
        .default_track(TrackType::Audio)
        .context("no audio track found in container")?;

    let params = match track.codec_params.as_ref() {
        Some(CodecParameters::Audio(params)) => params.clone(),
        _ => bail!("track is not audio"),
    };

    let dfla = params
        .extra_data
        .as_ref()
        .context("no FLAC extra data (dfLa box) found in mp4 track")?;

    let streaminfo = if dfla.len() == 34 {
        dfla.to_vec()
    } else {
        bail!("expected 34-byte STREAMINFO body, got {} bytes", dfla.len())
    };

    let out_file =
        File::create(output).with_context(|| format!("creating {}", output.display()))?;
    let mut writer = BufWriter::new(out_file);

    writer.write_all(b"fLaC")?;

    let len = streaminfo.len() as u32;
    let mut header = [0u8; 4];

    header[0] = 0x80;
    header[1] = ((len >> 16) & 0xFF) as u8;
    header[2] = ((len >> 8) & 0xFF) as u8;
    header[3] = (len & 0xFF) as u8;

    writer.write_all(&header)?;
    writer.write_all(&streaminfo)?;

    let track_id = track.id;
    while let Some(packet) = format.next_packet().context("reading mp4 packet")? {
        if packet.track_id != track_id {
            continue;
        }

        writer.write_all(packet.data.as_ref())?;
    }

    writer.flush()?;

    Ok(())
}
