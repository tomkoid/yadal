use tidlers::client::models::{album::AlbumResponse, track::Track};

use crate::downloader::utils::{CoverImage, TidalDateYmdhms};

#[derive(Debug, Clone)]
pub struct AlbumTagContext {
    pub id: String,
    pub title: String,
    pub explicit: bool,
    pub total_tracks: u32,
    pub total_discs: u32,
    pub artist: String,
    pub release_date: Option<String>,
    pub cover_uuid: Option<String>,
}

#[derive(Debug)]
pub struct TrackTagMetadata {
    pub title: String,
    pub tag_title: bool,
    pub track_version: Option<String>,
    pub track_number: Option<u32>,
    pub artists: Option<Vec<String>>,
    pub album_title: Option<String>,
    pub album_artists: Option<Vec<String>>,
    pub release_date: Option<String>,
    pub cover: Option<CoverImage>,
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
    pub url: Option<String>,
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
    TrackOnly(ReplayGainValues),
    AlbumOnly(ReplayGainValues),
    TrackAndAlbum(ReplayGainValues, ReplayGainValues),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReplayGainValues {
    pub gain: String,
    pub peak: String,
}

impl AlbumTagContext {
    pub fn from_album_response(album: &AlbumResponse) -> Self {
        Self {
            id: album.id.to_string(),
            explicit: album.explicit,
            total_tracks: album.number_of_tracks,
            total_discs: album.number_of_volumes,
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

#[derive(Debug, Clone)]
pub struct PlaylistTemplateContext {
    pub uuid: String,
    pub title: String,
    pub created: TidalDateYmdhms,
    pub last_updated: TidalDateYmdhms,
}

impl TrackTagMetadata {
    pub fn from_track(track: &Track, album: &AlbumTagContext) -> Self {
        let artists = if track.artists.is_empty() {
            vec![track.artist.name.clone()]
        } else {
            track
                .artists
                .iter()
                .map(|artist| artist.name.clone())
                .collect()
        };

        // temporary until tidlers implements getting multiple album artists
        let album_artists = Some(vec![album.artist.clone()]);

        Self {
            title: track.title.clone(),
            tag_title: false, // needs to be set by the consumer
            track_version: track.version.clone(),
            track_number: Some(track.track_number),
            artists: Some(artists),
            album_title: Some(album.title.clone()),
            album_artists,
            release_date: album.release_date.clone(),
            cover: None,
            lyrics: TagLyrics::None,
            bpm: track.bpm,
            key: track.key.clone(),
            key_scale: track.key_scale.clone(),
            copyright: track.copyright.clone(),
            disc_number: Some(track.volume_number),
            isrc: track.isrc.clone(),
            replaygain: TagReplayGain::None,
            total_discs: Some(album.total_discs),
            total_tracks: Some(album.total_tracks),
            url: Some(track.url.clone()),
        }
    }
}
