use std::{ops::RangeInclusive, path::PathBuf};

use tidlers::client::models::playback::AudioQuality;

use crate::config::{LyricsMode, ReplayGainMode};

#[derive(Debug, Clone)]
pub struct DownloaderConfig {
    pub download: DownloaderConfigDownload,
    pub tags: DownloaderConfigTags,
}

#[derive(Debug, Clone)]
pub struct DownloaderConfigDownload {
	pub output_path: PathBuf,
	pub output_template: String,
	pub audio_quality: AudioQuality,
	pub max_parallel: usize,
	pub force_download: bool,
	pub no_stream_check: bool,
	pub skip_transcode: bool,
    pub range: Option<RangeInclusive<usize>>,
}

#[derive(Debug, Clone)]
pub struct DownloaderConfigTags {
    pub enable: bool,
	pub album: bool,
	pub album_artist: bool,
	pub artist: bool,
	pub bpm: bool,
	pub copyright: bool,
	pub cover: bool,
	pub date: bool,
	pub disc_number: bool,
	pub isrc: bool,
	pub initial_key_and_key_scale: bool,
	pub lyrics: LyricsMode,
	pub replaygain: ReplayGainMode,
	pub title: bool,
	pub total_discs: bool,
	pub total_tracks: bool,
	pub version: bool,
	pub url: bool,
}
