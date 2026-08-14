use tidlers::TidalClient;

use crate::options::DownloaderOptions;

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
    http_client: reqwest::Client,
    options: DownloaderOptions,
}

impl Downloader {
    pub fn new(tidal_client: TidalClient, options: DownloaderOptions) -> Self {
        Self {
            tidal_client,
            http_client: reqwest::Client::new(),
            options,
        }
    }
}
