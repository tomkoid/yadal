use minijinja::{Environment, Output, State, Value};
use serde::Serialize;

#[derive(Serialize)]
pub struct TemplateContext<'a> {
    track: TemplateTrackCtx<'a>,
    album: TemplateAlbumCtx<'a>,
    playlist: TemplatePlaylistCtx<'a>,
}

#[derive(Serialize)]
pub struct TemplateTrackCtx<'a> {
    artists: Vec<&'a str>,
    bpm: Option<f32>,
    copyright: Option<&'a str>,
    explicit: bool,
    extension: &'a str,
    id: &'a str,
    isrc: Option<&'a str>,
    initial_key_and_key_scale: Option<&'a str>,
    number: u32,
    title: &'a str,
    version: Option<&'a str>,
    url: &'a str,
}

#[derive(Serialize)]
pub struct TemplateAlbumCtx<'a> {
    artists: Vec<&'a str>,
    explicit: bool,
    release: DateYmd,
    id: &'a str,
    title: &'a str,
    // temporary Option
    total_discs: Option<u32>,
    // temporary Option
    total_tracks: Option<u32>,
    url: &'a str,
}

#[derive(Serialize)]
pub struct TemplatePlaylistCtx<'a> {
    created: DateYmdhms,
    index: u32,
    title: &'a str,
    update: DateYmdhms,
    url: &'a str,
    uuid: &'a str,
}

#[derive(Serialize)]
pub struct DateYmd {
    day: Option<u64>,
    month: Option<u64>,
    year: u64,
}

#[derive(Serialize)]
pub struct DateYmdhms {
    day: u64,
    month: u64,
    year: u64,
    hour: u64,
    minute: u64,
    second: u64,
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
        let sanitized: String = rendered.chars()
            .map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') { '_' } else { c })
            .collect();

        write!(out, "{sanitized}").map_err(minijinja::Error::from)
    }

    pub fn make(template: &str) -> Result<Self, minijinja::Error> {
        let mut env = Environment::new();
        env.set_formatter(Self::sanitizing_formatter);
        env.add_template_owned("template", template.to_owned())?;

        Ok(Self {
            _env: env,
        })
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
