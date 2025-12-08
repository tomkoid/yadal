use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use tidlers::client::models::playback::AudioQuality;

mod args;
mod auth;
mod downloader;
mod types;

use auth::{authenticate, load_or_authenticate};
use downloader::Downloader;
use types::MediaType;

use crate::args::Cli;

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum, Debug)]
enum QualityArg {
    Low,
    High,
    Lossless,
    HiRes,
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
enum MediaTypeArg {
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

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // authenticate
    let mut client = if cli.reauth {
        println!("forcing re-authentication...\n");
        authenticate(&cli.session_file).await?
    } else {
        load_or_authenticate(&cli.session_file).await?
    };

    // set audio quality
    client.set_audio_quality(cli.quality.into());
    println!("audio quality: {:?}\n", cli.quality);

    // parse ID and determine media type
    let (media_id, detected_type) = parse_tidal_input(&cli.id);

    let media_type = match cli.media_type {
        MediaTypeArg::Auto => detected_type,
        MediaTypeArg::Track => MediaType::Track,
        MediaTypeArg::Album => MediaType::Album,
        MediaTypeArg::Playlist => MediaType::Playlist,
    };

    println!("media type: {:?}", media_type);
    println!("output directory: {}\n", cli.output.display());

    // create output directory
    std::fs::create_dir_all(&cli.output).context("Failed to create output directory")?;

    // create downloader
    let downloader = Downloader::new(cli.output, cli.parallel);

    // download based on type
    match media_type {
        MediaType::Track => {
            println!("downloading track {}...\n", media_id);
            downloader.download_track(&mut client, &media_id).await?;
        }
        MediaType::Album => {
            println!("downloading album {}...\n", media_id);
            downloader.download_album(&mut client, &media_id).await?;
        }
        MediaType::Playlist => {
            println!("downloading playlist {}...\n", media_id);
            downloader.download_playlist(&mut client, &media_id).await?;
        }
    }

    println!("\ndownload complete!");

    Ok(())
}

/// Parses TIDAL input (URL or ID) and returns (ID, MediaType)
///
/// Supports:
/// - https://tidal.com/track/437468401/u
/// - https://tidal.com/track/437468401
/// - https://tidal.com/album/55130630/u
/// - https://tidal.com/album/55130630
/// - https://tidal.com/playlist/aa692128-2954-4fe1-b5a1-4ede1add485d
/// - Raw IDs: 437468401, 55130630, aa692128-2954-4fe1-b5a1-4ede1add485d
fn parse_tidal_input(input: &str) -> (String, MediaType) {
    // Check if it's a URL
    if input.starts_with("http://") || input.starts_with("https://") {
        // Parse as URL
        if let Some(parsed) = parse_tidal_url(input) {
            return parsed;
        }
    }

    // Not a URL or failed to parse - treat as raw ID
    // Try to detect type from ID format
    if input.contains('-') {
        // UUIDs are typically playlists
        (input.to_string(), MediaType::Playlist)
    } else if input.parse::<u64>().is_ok() {
        // Numeric IDs - default to track
        (input.to_string(), MediaType::Track)
    } else {
        // Unknown format - default to track
        (input.to_string(), MediaType::Track)
    }
}

fn parse_tidal_url(url: &str) -> Option<(String, MediaType)> {
    // Remove trailing /u if present
    let url = url.trim_end_matches("/u").trim_end_matches('/');

    // Split by '/'
    let parts: Vec<&str> = url.split('/').collect();

    // URL format: https://tidal.com/{type}/{id}
    // We need at least [..., type, id]
    if parts.len() < 2 {
        return None;
    }

    // Get the last two parts (type and id)
    let id = parts[parts.len() - 1];
    let media_type_str = parts[parts.len() - 2];

    let media_type = match media_type_str {
        "track" => MediaType::Track,
        "album" => MediaType::Album,
        "playlist" => MediaType::Playlist,
        _ => return None,
    };

    Some((id.to_string(), media_type))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_track_url() {
        let (id, media_type) = parse_tidal_input("https://tidal.com/track/437468401");
        assert_eq!(id, "437468401");
        assert!(matches!(media_type, MediaType::Track));
    }

    #[test]
    fn test_parse_track_url_with_u() {
        let (id, media_type) = parse_tidal_input("https://tidal.com/track/437468401/u");
        assert_eq!(id, "437468401");
        assert!(matches!(media_type, MediaType::Track));
    }

    #[test]
    fn test_parse_album_url() {
        let (id, media_type) = parse_tidal_input("https://tidal.com/album/55130630");
        assert_eq!(id, "55130630");
        assert!(matches!(media_type, MediaType::Album));
    }

    #[test]
    fn test_parse_playlist_url() {
        let (id, media_type) =
            parse_tidal_input("https://tidal.com/playlist/aa692128-2954-4fe1-b5a1-4ede1add485d");
        assert_eq!(id, "aa692128-2954-4fe1-b5a1-4ede1add485d");
        assert!(matches!(media_type, MediaType::Playlist));
    }

    #[test]
    fn test_parse_numeric_id() {
        let (id, media_type) = parse_tidal_input("437468401");
        assert_eq!(id, "437468401");
        assert!(matches!(media_type, MediaType::Track));
    }

    #[test]
    fn test_parse_uuid_id() {
        let (id, media_type) = parse_tidal_input("aa692128-2954-4fe1-b5a1-4ede1add485d");
        assert_eq!(id, "aa692128-2954-4fe1-b5a1-4ede1add485d");
        assert!(matches!(media_type, MediaType::Playlist));
    }
}
