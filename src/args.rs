use std::path::PathBuf;

use clap::Parser;
use directories::ProjectDirs;

use crate::{MediaTypeArg, QualityArg};

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
    #[arg(short, long, value_enum, default_value = "hires")]
    pub quality: QualityArg,

    /// Output directory
    #[arg(short, long, default_value = "yadal")]
    pub output: PathBuf,

    /// Maximum parallel downloads
    #[arg(short, long, default_value = "5")]
    pub parallel: usize,

    /// Force re-authentication
    #[arg(long)]
    pub reauth: bool,

    /// Session file path
    #[arg(long, value_parser, default_value_os_t = default_session_file())]
    pub session_file: PathBuf,
}
