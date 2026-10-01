use minijinja::{Environment, Output, State, Value};
use std::path::PathBuf;

use serde::Serialize;
use tidlers::client::models::track::Track;

use crate::downloader::context::{AlbumTagContext, PlaylistTemplateContext};

#[derive(Serialize)]
pub struct TemplateContext<'a> {
    pub track: TemplateTrackCtx<'a>,
    pub album: TemplateAlbumCtx<'a>,
    pub playlist: Option<TemplatePlaylistCtx<'a>>,
}

#[derive(Serialize)]
pub struct TemplateTrackCtx<'a> {
    pub artists: Vec<&'a str>,
    pub bpm: Option<f32>,
    pub copyright: Option<&'a str>,
    pub explicit: bool,
    pub extension: &'a str,
    pub id: &'a str,
    pub isrc: Option<&'a str>,
    pub initial_key_and_key_scale: Option<String>,
    pub disc_number: u32,
    pub number: u32,
    pub title: &'a str,
    pub version: Option<&'a str>,
    pub url: &'a str,
}

#[derive(Serialize)]
pub struct TemplateAlbumCtx<'a> {
    pub artists: Vec<&'a str>,
    pub explicit: bool,
    pub release: Option<TidalDateYmd>,
    pub id: &'a str,
    pub title: &'a str,
    // temporary Option, not implemented in tidlers right now
    pub total_discs: Option<u32>,
    // temporary Option, not implemented in tidlers right now
    pub total_tracks: Option<u32>,
    pub url: String,
}

#[derive(Serialize)]
pub struct TemplatePlaylistCtx<'a> {
    pub created: Option<TidalDateYmdhms>,
    pub index: u32,
    pub title: &'a str,
    pub updated: Option<TidalDateYmdhms>,
    pub url: String,
    pub uuid: &'a str,
}

#[derive(Serialize)]
pub struct TidalDateYmd {
    pub day: Option<u64>,
    pub month: Option<u64>,
    pub year: u64,
}

#[derive(Serialize)]
pub struct TidalDateYmdhms {
    pub day: u64,
    pub month: u64,
    pub year: u64,
    pub hour: u64,
    pub minute: u64,
    pub second: u64,
}

pub struct Templater {
    _env: Environment<'static>,
}

impl Templater {
    fn sanitizing_formatter(
        out: &mut Output<'_>,
        _state: &State<'_, '_>,
        value: &Value,
    ) -> Result<(), minijinja::Error> {
        let rendered = value.to_string();
        let sanitized: String = rendered
            .chars()
            .map(|c| {
                if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                    '_'
                } else {
                    c
                }
            })
            .collect();

        write!(out, "{sanitized}").map_err(minijinja::Error::from)
    }

    pub fn make(template: &str) -> Result<Self, minijinja::Error> {
        let mut env = Environment::new();
        env.set_formatter(Self::sanitizing_formatter);
        env.add_template_owned("template", template.to_owned())?;

        Ok(Self { _env: env })
    }

    pub fn render(&self, ctx: &impl serde::Serialize) -> Result<String, minijinja::Error> {
        let template = self._env.get_template("template")?;

        template.render(ctx)
    }
}

pub fn validate(template: &str) -> Result<(), minijinja::Error> {
    let _ = Templater::make(template)?;

    Ok(())
}

fn parse_ymd(date: &str) -> Option<TidalDateYmd> {
    let mut parts = date.split('T').next()?.split('-');
    let year = parts.next()?.parse().ok()?;
    let month = parts.next().and_then(|m| m.parse().ok());
    let day = parts.next().and_then(|d| d.parse().ok());

    Some(TidalDateYmd { day, month, year })
}

/// parse the ISO 8601 timestamp returned by TIDAL for playlists \
/// important: it is NOT RFC 3339
fn parse_ymdhms(date: &str) -> Option<TidalDateYmdhms> {
    let (date_part, time_part) = date.split_once('T')?;

    let mut d = date_part.split('-');
    let year = d.next()?.parse().ok()?;
    let month = d.next()?.parse().ok()?;
    let day = d.next()?.parse().ok()?;

    // only keep HH:MM:SS
    let time_part = time_part.split(['.', '+', 'Z', '-']).next()?;
    let mut t = time_part.split(':');
    let hour = t.next()?.parse().ok()?;
    let minute = t.next()?.parse().ok()?;
    let second = t.next()?.parse().ok()?;

    Some(TidalDateYmdhms {
        day,
        month,
        year,
        hour,
        minute,
        second,
    })
}

pub fn render_track_path(
    templater: &Templater,
    track: &Track,
    album: Option<&AlbumTagContext>,
    playlist: Option<(&PlaylistTemplateContext, usize)>,
    extension: &str,
) -> Result<PathBuf, minijinja::Error> {
    let fallback;
    let album = match album {
        Some(album) => album,
        None => {
            fallback = AlbumTagContext::from_track(track);
            &fallback
        }
    };

    let track_id = track.id.to_string();
    let artists = if track.artists.is_empty() {
        vec![track.artist.name.as_str()]
    } else {
        track.artists.iter().map(|a| a.name.as_str()).collect()
    };
    let key = match (&track.key, &track.key_scale) {
        (Some(key), Some(scale)) => Some(format!("{key} {scale}")),
        (Some(key), None) => Some(key.clone()),
        _ => None,
    };

    let ctx = TemplateContext {
        track: TemplateTrackCtx {
            artists,
            bpm: track.bpm,
            copyright: track.copyright.as_deref(),
            explicit: track.explicit,
            extension,
            id: &track_id,
            isrc: track.isrc.as_deref(),
            initial_key_and_key_scale: key,
            disc_number: track.volume_number,
            number: track.track_number,
            title: &track.title,
            version: track.version.as_deref(),
            url: &track.url,
        },
        album: TemplateAlbumCtx {
            artists: vec![album.artist.as_str()],
            explicit: album.explicit,
            release: album.release_date.as_deref().and_then(parse_ymd),
            id: &album.id,
            title: &album.title,
            total_discs: None,
            total_tracks: album.total_tracks,
            url: format!("https://tidal.com/album/{}", album.id),
        },
        playlist: playlist.map(|(p, position)| TemplatePlaylistCtx {
            created: p.created.as_deref().and_then(parse_ymdhms),
            index: position as u32,
            title: &p.title,
            updated: p.last_updated.as_deref().and_then(parse_ymdhms),
            url: format!("https://tidal.com/playlist/{}", p.uuid),
            uuid: &p.uuid,
        }),
    };

    templater.render(&ctx).map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::parse_ymdhms;

    #[test]
    fn parses_tidal_timestamp_with_offset() {
        let d = parse_ymdhms("2024-03-05T12:34:56.000+0000").unwrap();

        assert_eq!(
            (d.year, d.month, d.day, d.hour, d.minute, d.second),
            (2024, 3, 5, 12, 34, 56)
        );
    }

    #[test]
    fn parses_timestamp_without_fraction() {
        let d = parse_ymdhms("2024-03-05T01:02:03Z").unwrap();

        assert_eq!((d.hour, d.minute, d.second), (1, 2, 3));
    }

    #[test]
    fn rejects_date_only() {
        assert!(parse_ymdhms("2024-03-05").is_none());
    }
}
