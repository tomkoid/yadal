use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use tidlers::TidalClient;

use crate::downloader::config::DownloaderConfig;
use crate::downloader::download::QueuedTrack;

pub mod config;
pub mod context;
pub mod download;
pub mod entries;
pub mod rate_limiter;
pub mod tagging;
pub mod ui;
pub mod utils;

/// Handles all download operations
pub struct Downloader {
    tidal_client: TidalClient,
    http_client: reqwest::Client,

    state: DownloaderState,
    config: DownloaderConfig,
}

impl Downloader {
    pub fn new(tidal_client: TidalClient, config: DownloaderConfig) -> Self {
        let state = DownloaderState::new();
        let http_client = reqwest::Client::new();

        Self {
            tidal_client,
            http_client,
            config,
            state,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DownloaderState {
    queued: Vec<QueuedTrack>,
    finished: Arc<AtomicUsize>,

    multi_progress: Option<indicatif::MultiProgress>,
    status_bar: Option<indicatif::ProgressBar>,
}

impl DownloaderState {
    pub fn new() -> Self {
        Self {
            queued: Vec::new(),
            finished: Arc::new(AtomicUsize::new(0)),
            multi_progress: None,
            status_bar: None,
        }
    }
}
