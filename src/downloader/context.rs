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
    pub tag_title: bool,
    pub track_number: Option<u32>,
    pub artists: Option<Vec<String>>,
    pub album_title: Option<String>,
    pub album_artists: Option<Vec<String>>,
    pub release_date: Option<String>,
    pub cover_url: Option<String>,
    pub lyrics: TagLyrics,
    pub bpm: Option<f32>,
    pub key: Option<String>,
    pub key_scale: Option<String>,
    pub copyright: Option<String>,
    pub disc_number: Option<u32>,
    pub isrc: Option<String>,
    pub replaygain: TagReplayGain,
    pub total_discs: Option<u32>,
    pub total_tracks: Option<u32>,
    pub url: Option<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TagLyrics {
	None,
	UnsyncedOnly(String),
	SyncedOnly(String),
	UnsyncedAndSynced(String, String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum TagReplayGain {
	None,
	TrackOnly(String),
	AlbumOnly(String),
	TrackAndAlbum(String, String),
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

        // temporary until tidlers implements getting multiple album artists
        let album_artist = if let Some(unwrapped_album_artist) = album_artist {
            Some(vec![unwrapped_album_artist])
        } else {
            None
        };

        let track_number = track.track_number;

        Self {
            title: track.title.clone(),
            tag_title: false, // needs to be set by the consumer
            track_number: Some(track_number),
            artists: Some(artists),
            album_title,
            album_artists: album_artist,
            release_date,
            cover_url,
            lyrics: TagLyrics::None,
            bpm: track.bpm,
            key: track.key.clone(),
            key_scale: track.key_scale.clone(),
            // fields past here are unfinished and can maybe be set from this function
            // this function might end up being changed altogether
            copyright: None,
            disc_number: None,
            isrc: None,
            replaygain: TagReplayGain::None,
            total_discs: None,
            total_tracks: None,
            url: None,
        }
    }
}
