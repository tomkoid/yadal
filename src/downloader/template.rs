use minijinja::{Environment, Output, State, Value};
use serde::Serialize;

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
    pub initial_key_and_key_scale: Option<&'a str>,
    pub number: u32,
    pub title: &'a str,
    pub version: Option<&'a str>,
    pub url: &'a str,
}

#[derive(Serialize)]
pub struct TemplateAlbumCtx<'a> {
    pub artists: Vec<&'a str>,
    pub explicit: bool,
    pub release: TidalDateYmd,
    pub id: &'a str,
    pub title: &'a str,
    // temporary Option
    pub total_discs: Option<u32>,
    // temporary Option
    pub total_tracks: Option<u32>,
    pub url: &'a str,
}

#[derive(Serialize)]
pub struct TemplatePlaylistCtx<'a> {
    pub created: TidalDateYmdhms,
    pub index: u32,
    pub title: &'a str,
    pub update: TidalDateYmdhms,
    pub url: &'a str,
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
