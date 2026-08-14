use std::path::PathBuf;

use tidlers::client::models::playback::AudioQuality;

use crate::types::MediaType;

#[derive(Debug, Clone)]
pub struct DownloaderConfig {
    pub media_type: MediaType,
    pub audio_quality: AudioQuality,
    pub max_parallel: usize,
    pub force_download: bool,
    pub output_path: PathBuf,
    pub skip_tag: bool,
}
