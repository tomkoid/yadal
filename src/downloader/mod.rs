use std::path::PathBuf;

use tidlers::{
    TidalClient,
    client::models::playback::{AudioQuality, PlaybackMode},
};

pub mod context;
pub mod download;
pub mod entries;
pub mod rate_limiter;
pub mod tagging;
pub mod ui;
pub mod utils;

/// Struct for handling all download operations
pub struct Downloader {
    tidal_client: TidalClient,
    output_dir: PathBuf,
    http_client: reqwest::Client,
    max_parallel: usize,
    audio_quality: AudioQuality,
}

impl Downloader {
    pub fn new(
        mut tidal_client: TidalClient,
        output_dir: PathBuf,
        audio_quality: AudioQuality,
        max_parallel: usize,
    ) -> Self {
        if audio_quality == AudioQuality::Lossless || audio_quality == AudioQuality::HiRes {
            tidal_client.set_playback_mode(PlaybackMode::Offline);
        }

        Self {
            tidal_client,
            output_dir,
            http_client: reqwest::Client::new(),
            max_parallel,
            audio_quality,
        }
    }
}
