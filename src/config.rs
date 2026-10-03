use serde::{Deserialize, Serialize};
use std::env::home_dir;
use std::io;
use std::{
    fs::create_dir_all,
    path::{Component, Path, PathBuf, Prefix},
};

use crate::{args::QualityArg, downloader::template::validate};

const OUTPUT_TEMPLATE: &str = "{{ album.artists[0] }}/{{ album.title }}{% if album.explicit %} (Explicit){% endif %}/{{ \"%02d\"|format(track.number) }} {{ track.title }}{% if track.version %} ({{ track.version }}){% endif %}{% if track.explicit %} (Explicit){% endif %}.{{ track.extension }}";

#[derive(Debug, thiserror::Error)]
pub enum FileConfigError {
    #[error("I/O Error: {0}")]
    IO(#[from] io::Error),
    #[error("TOML deserialising error: {0}")]
    TomlDeserialise(#[from] toml::de::Error),
    #[error("TOML serialising error: {0}")]
    TomlSerialise(#[from] toml::ser::Error),
    #[error("MiniJinja templating error: {0}")]
    MiniJinjaTemplate(#[from] minijinja::Error),
    #[error("replacing with home symbol failed: {0}")]
    ReplaceWithHomeSymbol(String),
    #[error("failed to obtain user audio directory")]
    CantGetAudioDir,
}

#[derive(Clone, Default, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct FileConfig {
    pub download: Download,
    pub tags: Tags,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Download {
    pub output_path: String,
    pub output_template: String,
    pub audio_quality: QualityArg,
    pub max_parallel: usize,
    pub force_download: bool,
    pub no_stream_check: bool,
    pub skip_transcode: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
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
    pub track_number: bool,
    pub track_version: bool,
    pub url: bool,
}

#[derive(Clone, Debug, PartialEq, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LyricsMode {
    None,
    UnsyncedOnly,
    SyncedOnly,
    #[default]
    UnsyncedAndSynced,
}

#[derive(Clone, Debug, PartialEq, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReplayGainMode {
    None,
    TrackOnly,
    AlbumOnly,
    #[default]
    TrackAndAlbum,
}

impl Default for Download {
    fn default() -> Self {
        Self {
            output_path: "REPLACE_THIS_VALUE".into(),
            output_template: OUTPUT_TEMPLATE.into(),
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
            track_number: true,
            track_version: true,
            url: true,
        }
    }
}

impl FileConfig {
    pub fn try_new() -> Result<Self, FileConfigError> {
        Self::get_config()
    }

    fn get_config() -> Result<Self, FileConfigError> {
        let config_path = Self::get_default_path()?;

        // check if config file exists
        if config_path.try_exists()? {
            Self::load_from_file(&config_path)
        } else {
            let mut config = Self::default();

            let audio_dir = if let Some(audio_dir) = dirs::audio_dir() {
                audio_dir.join("yadal")
            } else if let Some(home_dir) = home_dir() {
                home_dir.join("Music").join("yadal")
            } else {
                return Err(FileConfigError::CantGetAudioDir);
            };

            // creating a directory in here should be harmless, cause it would be created either way
            // it's a bit awkward from main if it's not created here...
            // it's created as .canonicalize() needs the path to exist
            if !audio_dir.try_exists()? {
                create_dir_all(&audio_dir)?
            }

            let audio_dir = audio_dir.canonicalize()?;

            config.download.output_path =
                if let Some(replaced_path) = replace_with_home_symbol(&audio_dir) {
                    replaced_path
                } else {
                    return Err(FileConfigError::ReplaceWithHomeSymbol(
                        audio_dir.display().to_string(),
                    ));
                };

            Ok(config)
        }
    }

    fn get_default_path() -> io::Result<PathBuf> {
        if let Some(config_dir) = dirs::config_dir() {
            Ok(config_dir.join("yadal").join("config.toml"))
        } else {
            Err(io::Error::other(
                "Failed to get user's config directory, something is wrong here.",
            ))
        }
    }

    fn load_from_file(path: &PathBuf) -> Result<FileConfig, FileConfigError> {
        let config_str = std::fs::read_to_string(path)?;
        let config: FileConfig = Self::from_toml(&config_str)?;

        validate(&config.download.output_template)?;

        Ok(config)
    }

    pub fn init_default_config() -> Result<PathBuf, FileConfigError> {
        let path = Self::get_default_path()?;

        let parent_folder = if let Some(parent) = path.parent() {
            parent
        } else {
            &path
        };

        if !parent_folder.try_exists()? {
            create_dir_all(parent_folder)?;
        }

        // this will error out if the config already exists, but like, why would you be running this command if it exists already
        let toml_str = toml::to_string(&Self::get_config()?)?;
        std::fs::write(&path, toml_str)?;

        Ok(path)
    }

    pub fn from_toml(toml_str: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(toml_str)
    }
}

fn component_key(c: Component<'_>) -> String {
    match c {
        // `C:`, `c:`, and `\\?\C:` all normalize to `C:`
        Component::Prefix(p) => match p.kind() {
            Prefix::Disk(d) | Prefix::VerbatimDisk(d) => {
                format!("{}:", (d as char).to_ascii_uppercase())
            }
            _ => p.as_os_str().to_string_lossy().to_lowercase(),
        },
        other => {
            let s = other.as_os_str().to_string_lossy();
            if cfg!(windows) {
                s.to_lowercase()
            } else {
                s.into_owned()
            }
        }
    }
}

/// converts a path like /home/user/Downloads or C:\Users\user\Downloads to ~/Downloads \
/// should be able to handle the majority of all paths
fn replace_with_home_symbol(path: &Path) -> Option<String> {
    let home = dirs::home_dir()?;

    let mut path_components = path.components();
    for home_component in home.components() {
        match path_components.next() {
            Some(c) if component_key(c) == component_key(home_component) => {}
            _ => return Some(path.display().to_string()),
        }
    }

    let rest = path_components
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");

    Some(if rest.is_empty() {
        "~".to_string()
    } else {
        format!("~/{rest}")
    })
}

pub fn expand_home_symbol(path: &str) -> Option<PathBuf> {
    if let Some(stripped) = path.strip_prefix("~/") {
        return Some(dirs::home_dir()?.join(stripped));
    } else if let Some(stripped) = path.strip_prefix("~\\") {
        return Some(dirs::home_dir()?.join(stripped));
    }

    Some(PathBuf::from(path))
}

#[test]
fn test_config_deserialization() {
    let toml_str = r#"
		[download]
		output_path = "/path/to/output"
		output_template = "{{ album.artists[0] }}/{{ album.title }}{% if album.explicit %} (Explicit){% endif %}/{{ \"%02d\"|format(track.number) }} {{ track.title }}{% if track.version %} ({{ track.version }}){% endif %}{% if track.explicit %} (Explicit){% endif %}.{{ track.extension }}"
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
		track_number = true
		track_version = true
		url = true
"#;

    let config: FileConfig = toml::from_str(toml_str).expect("Failed to deserialize config");
    let d = &config.download;
    let t = &config.tags;

    assert_eq!(d.output_path, String::from("/path/to/output"));
    assert_eq!(d.output_template, String::from(OUTPUT_TEMPLATE));
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
    assert!(t.track_number);
    assert!(t.track_version);
    assert!(t.url);
}
