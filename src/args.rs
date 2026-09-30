use std::{ops::RangeInclusive, path::PathBuf};

use clap::{Parser, ValueEnum, Subcommand};
use directories::ProjectDirs;
use tidlers::client::models::playback::AudioQuality;

use crate::config::LyricsMode;

fn default_session_file() -> PathBuf {
    ProjectDirs::from("", "", "yadal")
        .map(|proj_dirs| proj_dirs.data_dir().join("session.json"))
        .unwrap_or_else(|| PathBuf::from("session.json"))
}

#[derive(Debug, Parser)]
#[command(name = "tidal-downloader")]
#[command(author, version, about = "Download music from TIDAL", long_about = None)]
pub struct Cli {
    /// Enable tracing logs from Tidlers
    #[arg(long)]
    pub trace: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Generate a configuration file with everything explicitly set to the default
    ///
    /// Note that this file may become invalid in the future if left unchecked, it is better practice to remove
    /// config items that are their default values
    InitConfigFile,

    /// Download something from TIDAL
    Download{
        /// TIDAL URL or media ID (track, album, or playlist)
        ///
        /// Examples: \
        /// - https://tidal.com/track/437468401 \
        /// - https://tidal.com/album/55130630 \
        /// - https://tidal.com/playlist/aa692128-2954-4fe1-b5a1-4ede1add485d \
        /// - 437468401
        #[arg(value_name = "URL_OR_ID", required = true)]
        id: Vec<String>,

        /// Type of media to download
        #[arg(short, long, value_enum, default_value = "auto")]
        media_type: MediaTypeArg,

        /// Audio quality
        #[arg(short, long, value_enum)]
        quality: Option<QualityArg>,

        /// Output directory
        #[arg(short, long, default_value = None)]
        output: Option<PathBuf>,

        /// Output path template in the output directory
        #[arg(short, long, default_value = None)]
        template: Option<String>,

        /// Range (e.g., 1-10 for tracks 1 to 10, or 5 for track 5)
        #[arg(short, long, value_parser = parse_range)]
        range: Option<RangeInclusive<usize>>,

        /// Maximum parallel downloads
        #[arg(short, long)]
        parallel: Option<usize>,

        /// Force re-authentication
        #[arg(long)]
        reauth: bool,

        /// Use legacy OAuth2 device flow instead of PKCE
        #[arg(long)]
        oauth2: bool,

        /// Redownload even if matching local file exists
        #[arg(short, long, num_args = 0..=1, default_missing_value = "true")]
        force: Option<bool>,

        /// Skip checking if the stream is available before downloading
        #[arg(long, num_args = 0..=1, default_missing_value = "true")]
        no_stream_check: Option<bool>,

        /// Skip tagging
        #[arg(short, long, num_args = 0..=1, default_missing_value = "true")]
        skip_tag: Option<bool>,

        /// Add lyrics to the downloaded files (if available)
        #[arg(short, long, num_args = 0..=1, default_missing_value = "true")]
        lyrics: Option<Lyrics>,

        /// Skip transcoding and use the original file (m4a most of the time)
        #[arg(long, num_args = 0..=1, default_missing_value = "true")]
        skip_transcode: Option<bool>,

        /// Session file path
        #[arg(long, value_parser, default_value_os_t = default_session_file())]
        session_file: PathBuf,
    },
}

#[derive(
    Copy, Clone, PartialEq, Eq, ValueEnum, Debug, Default, serde::Deserialize, serde::Serialize,
)]
#[serde(rename_all = "kebab-case")]
pub enum QualityArg {
    Low,
    High,
    Lossless,
    #[default]
    HiRes,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, ValueEnum)]
pub enum MediaTypeArg {
    Auto,
    Track,
    Album,
    Playlist,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, ValueEnum)]
pub enum Lyrics {
    None,
    UnsyncedOnly,
    SyncedOnly,
    UnsyncedAndSynced,
}

impl From<QualityArg> for AudioQuality {
    fn from(val: QualityArg) -> Self {
        match val {
            QualityArg::Low => AudioQuality::Low,
            QualityArg::High => AudioQuality::High,
            QualityArg::Lossless => AudioQuality::Lossless,
            QualityArg::HiRes => AudioQuality::HiResLossless,
        }
    }
}

impl From<Lyrics> for LyricsMode {
    fn from(val: Lyrics) -> Self {
        match val {
            Lyrics::None => LyricsMode::None,
            Lyrics::UnsyncedOnly => LyricsMode::UnsyncedOnly,
            Lyrics::SyncedOnly => LyricsMode::SyncedOnly,
            Lyrics::UnsyncedAndSynced => LyricsMode::UnsyncedAndSynced,
        }
    }
}

impl From<LyricsMode> for Lyrics {
    fn from(val: LyricsMode) -> Self {
        match val {
            LyricsMode::None => Lyrics::None,
            LyricsMode::UnsyncedOnly => Lyrics::UnsyncedOnly,
            LyricsMode::SyncedOnly => Lyrics::SyncedOnly,
            LyricsMode::UnsyncedAndSynced => Lyrics::UnsyncedAndSynced,
        }
    }
}

fn parse_range(s: &str) -> Result<RangeInclusive<usize>, String> {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 2 {
        return Err("Range must be in the format 'start-end' (e.g., 1-10)".to_string());
    }

    let start = parts[0]
        .parse::<usize>()
        .map_err(|_| format!("Invalid start integer: '{}'", parts[0]))?;
    let end = parts[1]
        .parse::<usize>()
        .map_err(|_| format!("Invalid end integer: '{}'", parts[1]))?;

    if start > end {
        return Err(format!(
            "Start ({}) cannot be greater than end ({})",
            start, end
        ));
    }

    Ok(start..=end)
}
