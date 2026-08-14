use crate::types::MediaType;

pub struct Target {
    pub id: String,
    pub media_type: MediaType,
}

impl Target {
    pub fn new(id: String, media_type: MediaType) -> Self {
        Self { id, media_type }
    }
}

/// Parses TIDAL input (URL or ID) and returns Vec<Media>
///
/// Supports:
/// - https://tidal.com/track/437468401/u
/// - https://tidal.com/track/437468401
/// - https://tidal.com/album/55130630/u
/// - https://tidal.com/album/55130630
/// - https://tidal.com/playlist/aa692128-2954-4fe1-b5a1-4ede1add485d
/// - Raw IDs: 437468401, 55130630, aa692128-2954-4fe1-b5a1-4ede1add485d
pub fn parse_id_input(input: &str) -> Vec<Target> {
    let input_target = input
        .trim()
        .split(",")
        .map(|s| s.trim().to_string())
        .collect::<Vec<String>>();

    let mut media: Vec<Target> = Vec::new();

    for id in &input_target {
        // Check if it's a URL
        if id.starts_with("http://") || id.starts_with("https://") {
            // Parse as URL
            if let Some(parsed) = parse_tidal_url(id) {
                media.push(Target::new(parsed.0, parsed.1));
                continue;
            }
        }

        // Not a URL or failed to parse - treat as raw ID
        // Try to detect type from ID format
        let media_id = if id.contains('-') {
            match id.contains("upload/") {
                true => Target::new(id.to_string(), MediaType::Track), // Uploaded tracks are typically tracks
                false => Target::new(id.to_string(), MediaType::Playlist), // Other UUIDs are typically playlists
            }
        } else if id.parse::<u64>().is_ok() {
            // Numeric IDs - default to track
            Target::new(id.to_string(), MediaType::Track)
        } else {
            // Unknown format - default to track
            Target::new(id.to_string(), MediaType::Track)
        };

        media.push(media_id);
    }

    media
}

pub fn parse_tidal_url(url: &str) -> Option<(String, MediaType)> {
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
        "upload" => MediaType::Track,
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
        let targets = parse_id_input("https://tidal.com/track/437468401");
        assert_eq!(targets.len(), 1);
        let target = &targets[0];
        assert_eq!(target.id, "437468401");
        assert!(matches!(target.media_type, MediaType::Track));
    }

    #[test]
    fn test_parse_track_url_with_u() {
        let targets = parse_id_input("https://tidal.com/track/437468401/u");
        assert_eq!(targets.len(), 1);
        let target = &targets[0];
        assert_eq!(target.id, "437468401");
        assert!(matches!(target.media_type, MediaType::Track));
    }

    #[test]
    fn test_parse_album_url() {
        let targets = parse_id_input("https://tidal.com/album/55130630");
        assert_eq!(targets.len(), 1);
        let target = &targets[0];
        assert_eq!(target.id, "55130630");
        assert!(matches!(target.media_type, MediaType::Album));
    }

    #[test]
    fn test_parse_playlist_url() {
        let targets = parse_id_input("https://tidal.com/playlist/aa692128-2954-4fe1-b5a1-4ede1add485d");
        assert_eq!(targets.len(), 1);
        let target = &targets[0];
        assert_eq!(target.id, "aa692128-2954-4fe1-b5a1-4ede1add485d");
        assert!(matches!(target.media_type, MediaType::Playlist));
    }

    #[test]
    fn test_parse_numeric_id() {
        let targets = parse_id_input("437468401");
        assert_eq!(targets.len(), 1);
        let target = &targets[0];
        assert_eq!(target.id, "437468401");
        assert!(matches!(target.media_type, MediaType::Track));
    }

    #[test]
    fn test_parse_uuid_id() {
        let targets = parse_id_input("aa692128-2954-4fe1-b5a1-4ede1add485d");
        assert_eq!(targets.len(), 1);
        let target = &targets[0];
        assert_eq!(target.id, "aa692128-2954-4fe1-b5a1-4ede1add485d");
        assert!(matches!(target.media_type, MediaType::Playlist));
    }

    #[test]
    fn test_parse_multiple_targets() {
        let input = "437468401, aa692128-2954-4fe1-b5a1-4ede1add485d, https://tidal.com/album/55130630";
        let targets = parse_id_input(input);
        assert_eq!(targets.len(), 3);

        assert_eq!(targets[0].id, "437468401");
        assert!(matches!(targets[0].media_type, MediaType::Track));

        assert_eq!(targets[1].id, "aa692128-2954-4fe1-b5a1-4ede1add485d");
        assert!(matches!(targets[1].media_type, MediaType::Playlist));

        assert_eq!(targets[2].id, "55130630");
        assert!(matches!(targets[2].media_type, MediaType::Album));
    }
}

