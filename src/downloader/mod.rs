use std::path::PathBuf;

pub mod context;
pub mod download;
pub mod entries;
pub mod rate_limiter;
pub mod tagging;
pub mod ui;
pub mod utils;

/// Struct for handling all download operations
pub struct Downloader {
    output_dir: PathBuf,
    http_client: reqwest::Client,
    max_parallel: usize,
}

impl Downloader {
    pub fn new(output_dir: PathBuf, max_parallel: usize) -> Self {
        Self {
            output_dir,
            http_client: reqwest::Client::new(),
            max_parallel,
        }
    }
}
