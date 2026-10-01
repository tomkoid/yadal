use tidlers::TidalClient;

use crate::downloader::config::DownloaderConfig;

pub mod config;
pub mod context;
pub mod download;
pub mod entries;
pub mod rate_limiter;
pub mod tagging;
pub mod template;
pub mod ui;
pub mod utils;

/// Handles all download operations
pub struct Downloader {
    tidal_client: TidalClient,
    http_client: reqwest::Client,
    config: DownloaderConfig,
}

impl Downloader {
    pub fn new(tidal_client: TidalClient, config: DownloaderConfig) -> Self {
        let http_client = reqwest::Client::new();

        Self {
            tidal_client,
            http_client,
            config,
        }
    }
}
