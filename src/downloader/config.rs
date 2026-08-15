use std::path::PathBuf;

use tidlers::client::models::playback::AudioQuality;

#[derive(Debug, Clone)]
pub struct DownloaderConfig {
    pub audio_quality: AudioQuality,
    pub max_parallel: usize,
    pub force_download: bool,
    pub output_path: PathBuf,
    pub skip_tag: bool,
    pub skip_transcode: bool,
    pub lyrics: bool,
}
