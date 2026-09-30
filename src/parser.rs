use std::sync::LazyLock;

use regex::Regex;

use crate::types::MediaType;

const TIDAL_REGEX_UUID: &str = r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}";
static TIDAL_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"(?xi)
        ^(?:
            (?:https?://)?(?:www\.|listen\.)?tidal\.com/(?:browse/)?
            (?P<kind>track|album|playlist)/(?P<id>\d+|{TIDAL_REGEX_UUID})
            (?:/u|\?.*)?/?
          |
            (?P<raw>\d+|{TIDAL_REGEX_UUID})
        )$"
    ))
    .unwrap()
});

pub struct Target {
    pub id: String,
    pub media_type: MediaType,
}

impl Target {
    pub fn new(id: String, media_type: MediaType) -> Self {
        Self { id, media_type }
    }
}

fn parse_one(value: &str) -> Option<Target> {
    let caps = TIDAL_REGEX.captures(value)?;

    if let Some(raw) = caps.name("raw") {
        let typ = if raw.as_str().contains('-') {
            MediaType::Playlist
        } else {
            MediaType::Track
        };

        return Some(Target::new(raw.as_str().to_owned(), typ));
    }

    let typ = match caps["kind"].to_ascii_lowercase().as_str() {
        "track" => MediaType::Track,
        "album" => MediaType::Album,
        "playlist" => MediaType::Playlist,
        _ => return None,
    };

    Some(Target::new(caps["id"].to_owned(), typ))
}

/// Parses TIDAL input (URL or ID) and returns Vec<Media>
///
/// Supports nearly all formats in a TIDAL URL, as well as a raw ID/UUID
pub fn parse_id_input<S: AsRef<str>>(input: &[S]) -> Option<Vec<Target>> {
    input
        .into_iter()
        .map(|s| parse_one(s.as_ref().trim()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_track_url() {
        let targets = parse_id_input(&["https://tidal.com/track/437468401"]);
        assert!(targets.is_some());
        let targets = targets.unwrap();
        assert_eq!(targets.len(), 1);
        let target = &targets[0];
        assert_eq!(target.id, "437468401");
        assert!(matches!(target.media_type, MediaType::Track));
    }

    #[test]
    fn test_parse_track_url_with_universal_marker_slash() {
        let targets = parse_id_input(&["https://tidal.com/track/437468401/u"]);
        assert!(targets.is_some());
        let targets = targets.unwrap();
        assert_eq!(targets.len(), 1);
        let target = &targets[0];
        assert_eq!(target.id, "437468401");
        assert!(matches!(target.media_type, MediaType::Track));
    }

    #[test]
    fn test_parse_track_url_with_universal_marker_question_mark() {
        let targets = parse_id_input(&["https://tidal.com/track/437468401?u"]);
        assert!(targets.is_some());
        let targets = targets.unwrap();
        assert_eq!(targets.len(), 1);
        let target = &targets[0];
        assert_eq!(target.id, "437468401");
        assert!(matches!(target.media_type, MediaType::Track));
    }

    #[test]
    fn test_parse_album_url() {
        let targets = parse_id_input(&["https://tidal.com/album/55130630"]);
        assert!(targets.is_some());
        let targets = targets.unwrap();
        assert_eq!(targets.len(), 1);
        let target = &targets[0];
        assert_eq!(target.id, "55130630");
        assert!(matches!(target.media_type, MediaType::Album));
    }

    #[test]
    fn test_parse_album_url_with_universal_marker_slash() {
        let targets = parse_id_input(&["https://tidal.com/album/55130630/u"]);
        assert!(targets.is_some());
        let targets = targets.unwrap();
        assert_eq!(targets.len(), 1);
        let target = &targets[0];
        assert_eq!(target.id, "55130630");
        assert!(matches!(target.media_type, MediaType::Album));
    }

    #[test]
    fn test_parse_album_url_with_universal_marker_question_mark() {
        let targets = parse_id_input(&["https://tidal.com/album/55130630?u"]);
        assert!(targets.is_some());
        let targets = targets.unwrap();
        assert_eq!(targets.len(), 1);
        let target = &targets[0];
        assert_eq!(target.id, "55130630");
        assert!(matches!(target.media_type, MediaType::Album));
    }

    #[test]
    fn test_parse_playlist_url() {
        let targets =
            parse_id_input(&["https://tidal.com/playlist/aa692128-2954-4fe1-b5a1-4ede1add485d"]);
        assert!(targets.is_some());
        let targets = targets.unwrap();
        assert_eq!(targets.len(), 1);
        let target = &targets[0];
        assert_eq!(target.id, "aa692128-2954-4fe1-b5a1-4ede1add485d");
        assert!(matches!(target.media_type, MediaType::Playlist));
    }

    #[test]
    fn test_parse_playlist_url_with_universal_marker_question_mark() {
        let targets =
            parse_id_input(&["https://tidal.com/playlist/aa692128-2954-4fe1-b5a1-4ede1add485d?u"]);
        assert!(targets.is_some());
        let targets = targets.unwrap();
        assert_eq!(targets.len(), 1);
        let target = &targets[0];
        assert_eq!(target.id, "aa692128-2954-4fe1-b5a1-4ede1add485d");
        assert!(matches!(target.media_type, MediaType::Playlist));
    }

    #[test]
    fn test_parse_numeric_id() {
        let targets = parse_id_input(&["437468401"]);
        assert!(targets.is_some());
        let targets = targets.unwrap();
        assert_eq!(targets.len(), 1);
        let target = &targets[0];
        assert_eq!(target.id, "437468401");
        assert!(matches!(target.media_type, MediaType::Track));
    }

    #[test]
    fn test_parse_uuid_id() {
        let targets = parse_id_input(&["aa692128-2954-4fe1-b5a1-4ede1add485d"]);
        assert!(targets.is_some());
        let targets = targets.unwrap();
        assert_eq!(targets.len(), 1);
        let target = &targets[0];
        assert_eq!(target.id, "aa692128-2954-4fe1-b5a1-4ede1add485d");
        assert!(matches!(target.media_type, MediaType::Playlist));
    }

    #[test]
    fn test_parse_multiple_targets() {
        let input = [
            "437468401",
            "aa692128-2954-4fe1-b5a1-4ede1add485d",
            "https://tidal.com/album/55130630",
        ];
        let targets = parse_id_input(&input);
        assert!(targets.is_some());
        let targets = targets.unwrap();
        assert_eq!(targets.len(), 3);

        assert_eq!(targets[0].id, "437468401");
        assert!(matches!(targets[0].media_type, MediaType::Track));

        assert_eq!(targets[1].id, "aa692128-2954-4fe1-b5a1-4ede1add485d");
        assert!(matches!(targets[1].media_type, MediaType::Playlist));

        assert_eq!(targets[2].id, "55130630");
        assert!(matches!(targets[2].media_type, MediaType::Album));
    }
}
