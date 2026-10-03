use std::{collections::HashMap, sync::Arc};

use tidlers::{TidalClient, client::models::album::AlbumResponse};
use tokio::sync::Mutex;

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
    album_cache: Mutex<HashMap<String, Arc<AlbumResponse>>>,
}

impl Downloader {
    pub fn new(tidal_client: TidalClient, config: DownloaderConfig) -> Self {
        let http_client = reqwest::Client::new();

        Self {
            tidal_client,
            http_client,
            config,
            album_cache: Mutex::new(HashMap::new()),
        }
    }
}
