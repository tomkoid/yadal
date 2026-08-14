use std::path::PathBuf;

use clap::{Parser, ValueEnum};
use directories::ProjectDirs;
use tidlers::client::models::playback::AudioQuality;

fn default_session_file() -> PathBuf {
    ProjectDirs::from("", "", "yadal")
        .map(|proj_dirs| proj_dirs.data_dir().join("session.json"))
        .unwrap_or_else(|| PathBuf::from("session.json"))
}

#[derive(Parser)]
#[command(name = "tidal-downloader")]
#[command(author, version, about = "Download music from TIDAL", long_about = None)]
pub struct Cli {
    /// TIDAL URL or media ID (track, album, or playlist)
    ///
    /// Examples:
    ///   https://tidal.com/track/437468401
    ///   https://tidal.com/album/55130630
    ///   https://tidal.com/playlist/aa692128-2954-4fe1-b5a1-4ede1add485d
    ///   437468401
    #[arg(value_name = "URL_OR_ID")]
    pub id: String,

    /// Type of media to download
    #[arg(short, long, value_enum, default_value = "auto")]
    pub media_type: MediaTypeArg,

    /// Audio quality
    #[arg(short, long, value_enum, default_value = "hi-res")]
    pub quality: QualityArg,

    /// Output directory
    #[arg(short, long, default_value = None)]
    pub output: Option<PathBuf>,

    /// Maximum parallel downloads
    #[arg(short, long, default_value = "5")]
    pub parallel: usize,

    /// Force re-authentication
    #[arg(long)]
    pub reauth: bool,

    /// Use legacy OAuth2 device flow instead of PKCE
    #[arg(long)]
    pub oauth2: bool,

    /// Redownload even if matching local file exists
    #[arg(short, long)]
    pub force: bool,

    /// Skip tagging
    #[arg(short, long)]
    pub skip_tag: bool,

    /// Session file path
    #[arg(long, value_parser, default_value_os_t = default_session_file())]
    pub session_file: PathBuf,
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum, Debug)]
pub enum QualityArg {
    Low,
    High,
    Lossless,
    HiRes,
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
pub enum MediaTypeArg {
    Auto,
    Track,
    Album,
    Playlist,
}

impl From<QualityArg> for AudioQuality {
    fn from(val: QualityArg) -> Self {
        match val {
            QualityArg::Low => AudioQuality::Low,
            QualityArg::High => AudioQuality::High,
            QualityArg::Lossless => AudioQuality::Lossless,
            QualityArg::HiRes => AudioQuality::HiRes,
        }
    }
}
