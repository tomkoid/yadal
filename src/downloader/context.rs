use tidlers::{
    client::models::{album::AlbumResponse, track::Track},
    resources::uuid_to_url_with_size,
};

#[derive(Clone)]
pub struct AlbumTagContext {
    pub title: String,
    pub artist: String,
    pub release_date: Option<String>,
    pub cover_uuid: Option<String>,
}

pub struct TrackTagMetadata {
    pub title: String,
    pub track_number: u32,
    pub artists: Vec<String>,
    pub album_title: Option<String>,
    pub album_artist: Option<String>,
    pub release_date: Option<String>,
    pub cover_url: Option<String>,
}

impl AlbumTagContext {
    pub fn from_album_response(album: &AlbumResponse) -> Self {
        Self {
            title: album.title.clone(),
            artist: album.artist.name.clone(),
            release_date: Some(album.release_date.clone()),
            cover_uuid: if album.cover.trim().is_empty() {
                None
            } else {
                Some(album.cover.clone())
            },
        }
    }
}

impl TrackTagMetadata {
    pub fn from_track(track: &Track, album_context: Option<AlbumTagContext>) -> Self {
        let artists = if track.artists.is_empty() {
            vec![track.artist.name.clone()]
        } else {
            track
                .artists
                .iter()
                .map(|artist| artist.name.clone())
                .collect()
        };

        let (album_title, album_artist, release_date, cover_url) = match album_context {
            Some(context) => (
                Some(context.title),
                Some(context.artist),
                context.release_date,
                context
                    .cover_uuid
                    .as_deref()
                    .map(|uuid| uuid_to_url_with_size(uuid, 1280))
                    .or_else(|| {
                        track
                            .album
                            .as_ref()
                            .unwrap()
                            .cover
                            .as_deref()
                            .map(|uuid| uuid_to_url_with_size(uuid, 1280))
                    }),
            ),
            None => (
                Some(track.album.as_ref().unwrap().title.clone()),
                Some(track.artist.name.clone()),
                track.album.as_ref().unwrap().release_date.clone(),
                track
                    .album
                    .as_ref()
                    .unwrap()
                    .cover
                    .as_deref()
                    .map(|uuid| uuid_to_url_with_size(uuid, 1280)),
            ),
        };

        let track_number = track.track_number;

        Self {
            title: track.title.clone(),
            track_number,
            artists,
            album_title,
            album_artist,
            release_date,
            cover_url,
        }
    }
}
