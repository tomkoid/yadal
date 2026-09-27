use serde::{Deserialize, Serialize};
use std::{fs::create_dir_all, path::PathBuf};

use crate::args::QualityArg;

#[derive(Deserialize, Serialize)]
#[serde(default)]
pub struct FileConfig {
	pub download: Download,
	pub tags: Tags,
}

#[derive(Deserialize, Serialize)]
#[serde(default)]
pub struct Download {
	pub output_path: PathBuf,
	pub output_template: String,
	pub audio_quality: QualityArg,
	pub max_parallel: usize,
	pub force_download: bool,
	pub no_stream_check: bool,
	pub skip_transcode: bool,
}

#[derive(Deserialize, Serialize)]
#[serde(default)]
pub struct Tags {
	pub enable: bool,
	pub album: bool,
	pub album_artist: bool,
	pub artist: bool,
	pub bpm: bool,
	pub copyright: bool,
	pub cover: bool, // cover is not technically a tag in some containers like FLAC, but for most it is
	pub date: bool,
	pub disc_number: bool,
	pub isrc: bool,
	pub initial_key_and_key_scale: bool,
	pub lyrics: LyricsMode,
	pub replaygain: ReplayGainMode,
	pub title: bool,
	pub total_discs: bool,
	pub total_tracks: bool,
	pub url: bool,
}

#[derive(Debug, PartialEq, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LyricsMode {
	None,
	UnsyncedOnly,
	SyncedOnly,
	#[default]
	UnsyncedAndSynced,
}

#[derive(Debug, PartialEq, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReplayGainMode {
	None,
	TrackOnly,
	AlbumOnly,
	#[default]
	TrackAndAlbum,
}

impl Default for FileConfig {
	fn default() -> Self {
		FileConfig {
			download: Download::default(),
			tags: Tags::default(),
		}
	}
}

impl Default for Download {
	fn default() -> Self {
		let config_dir = dirs::home_dir().expect("Failed to get user's config directory");
		let default_output_path = config_dir.join("Music").join("yadal");

		Self {
			output_path: default_output_path,
			output_template:
				"{album.artist}/{album.title}/{album.index} {track.title} ({track.version}){\" (Explicit)\" if track.explicit else \"\"}.{track.extension}".into(),
			audio_quality: QualityArg::default(),
			max_parallel: 5,
			force_download: false,
			no_stream_check: false,
			skip_transcode: false,
		}
	}
}

impl Default for Tags {
	fn default() -> Self {
		Self {
			enable: true,
			album: true,
			album_artist: true,
			artist: true,
			bpm: true,
			copyright: true,
			cover: true,
			date: true,
			disc_number: true,
			isrc: true,
			initial_key_and_key_scale: true,
			lyrics: LyricsMode::default(),
			replaygain: ReplayGainMode::default(),
			title: true,
			total_discs: true,
			total_tracks: true,
			url: true,
		}
	}
}

impl FileConfig {
	pub fn try_new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
		let config = FileConfig::default();
		config.get_config()
	}

	fn get_config(&self) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
		let config_path = Self :: get_default_path();

		// check if config file exists
		if config_path.try_exists()? {
			return self.load_from_file(&config_path);
		} else {
			return Ok(Self::default());
		}
	}

	fn get_default_path() -> PathBuf {
		let config_dir = dirs::config_dir();

		if let Some(config_dir) = config_dir {
			return config_dir.join("yadal").join("config.toml");
		} else {
			panic!("Failed to get user's config directory, something is wrong here.");
		}
	}
	//
	fn load_from_file(&self, path: &PathBuf) -> Result<FileConfig, Box<dyn std::error::Error + Send + Sync>> {
		let config_str = std::fs::read_to_string(path)?;
		let config: FileConfig = self.from_toml(&config_str)?;
		Ok(config)
	}

	pub fn init_default_config() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
		let path = Self::get_default_path();

		let parent_folder = if let Some(parent) = path.parent() {
			parent
		} else {
			&path
		};

		if !parent_folder.try_exists()? {
			create_dir_all(parent_folder)?;
		}

		let toml_str = toml::to_string(&Self::default())?;
		std::fs::write(path, toml_str)?;
		Ok(())
	}

	pub fn from_toml(&self, toml_str: &str) -> Result<Self, toml::de::Error> {
		toml::from_str(toml_str)
	}

	pub fn parse_output_template(&self) -> String {
		"temporary value".into()
	}
}

#[test]
fn test_config_deserialization() {
	let toml_str = r#"
		[download]
		output_path = "/path/to/output"
		output_template = "{album.artist}/{album.title}/{album.index} {track.title} ({track.version}){\" (Explicit)\" if track.explicit else \"\"}.{track.extension}"
		audio_quality = "high"
		max_parallel = 5
		force_download = false
		no_stream_check = false
		skip_transcode = false

		[tags]
		enable = true
		album = true
		album_artist = true
		artist = true
		bpm = true
		copyright = true
		cover = true
		date = true
		disc_number = true
		isrc = true
		initial_key_and_key_scale = true
		lyrics = "unsynced-and-synced"
		replaygain = "track-and-album"
		title = true
		total_discs = true
		total_tracks = true
		url = true
	"#;

	let config: FileConfig = toml::from_str(toml_str).expect("Failed to deserialize config");
	let d = &config.download;
	let t = &config.tags;

	assert_eq!(d.output_path, PathBuf::from("/path/to/output"));
	assert_eq!(d.audio_quality, QualityArg::High);
	assert_eq!(d.max_parallel, 5);
	assert!(!d.force_download);
	assert!(!d.no_stream_check);
	assert!(!d.skip_transcode);
	assert!(t.enable);
	assert!(t.album);
	assert!(t.album_artist);
	assert!(t.artist);
	assert!(t.bpm);
	assert!(t.copyright);
	assert!(t.cover);
	assert!(t.date);
	assert!(t.disc_number);
	assert!(t.initial_key_and_key_scale);
	assert!(t.isrc);
	assert_eq!(t.lyrics, LyricsMode::UnsyncedAndSynced);
	assert_eq!(t.replaygain, ReplayGainMode::TrackAndAlbum);
	assert!(t.title);
	assert!(t.total_discs);
	assert!(t.total_tracks);
	assert!(t.url);
}
