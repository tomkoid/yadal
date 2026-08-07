use std::time::Duration;

use indicatif::{ProgressBar, ProgressStyle};

use crate::downloader::Downloader;

impl Downloader {
    pub fn create_spinner(&self, message: &str) -> ProgressBar {
        let pb = ProgressBar::new_spinner();
        pb.enable_steady_tick(Duration::from_millis(100));
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner} [{elapsed_precise}] {msg}")
                .unwrap(),
        );
        pb.set_message(message.to_string());
        pb
    }
}
