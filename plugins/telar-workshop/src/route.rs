//! [`WorkshopRoute`]: where the workshop is, as an address carries it — `/preview/<id>` or `/docs/<title>` — and what a copied link adds to that: the args edited on the preview and the canvas toolbar's settings, `?args=…&globals=…`, and `?chrome=0` for the canvas alone.
//!
//! The args travel as `name:value;…`, each value in [`ArgValue`]'s text form, and the settings as `key:value;…` words, settings at their defaults and args at theirs adding nothing. A route carries them only while a link that arrived is being opened: once they are set the workshop stands on its page alone, so the address it reports as it moves never holds an edited arg.

use std::sync::Arc;

use telar::preview::{
    ArgValue, control_size_word, direction_word, parse_control_size, parse_direction, parse_size,
};
use telar::{Location, Route};

use crate::settings::{Background, CanvasSettings, CanvasSize, Zoom};

const PREVIEW: &str = "preview";
const DOCS: &str = "docs";
const ARGS: &str = "args";
const GLOBALS: &str = "globals";
const CHROME: &str = "chrome";

/// Which page the workshop shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Page {
    /// A preview on its canvas, by id.
    Preview(Arc<str>),
    /// A component's docs, by the title its previews are listed under.
    Docs(Arc<str>),
}

/// What a link sets as it arrives.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Link {
    /// The args that differ from their defaults, by name.
    pub(crate) args: Vec<(String, ArgValue)>,
    /// The canvas toolbar's settings, whole. `None` leaves them as they are.
    pub(crate) settings: Option<CanvasSettings>,
}

/// Where the workshop is: a page, whether its chrome shows, and what a link that arrived there still has to set.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WorkshopRoute {
    pub(crate) page: Page,
    pub(crate) chrome: bool,
    pub(crate) link: Option<Link>,
}

impl WorkshopRoute {
    pub(crate) fn new(page: Page, chrome: bool) -> Self {
        Self {
            page,
            chrome,
            link: None,
        }
    }
}

impl Route for WorkshopRoute {
    fn to_location(&self) -> Location {
        let location = match &self.page {
            Page::Preview(id) => Location::from_segments([PREVIEW, id.as_ref()]),
            Page::Docs(title) => Location::from_segments(
                std::iter::once(DOCS).chain(title.split('/').map(str::trim)),
            ),
        };
        let mut location = match self.chrome {
            true => location,
            false => location.with_param(CHROME, "0"),
        };
        let Some(link) = &self.link else {
            return location;
        };
        if !link.args.is_empty() {
            location = location.with_param(ARGS, args_text(&link.args));
        }
        if let Some(settings) = link
            .settings
            .as_ref()
            .filter(|settings| **settings != CanvasSettings::default())
        {
            location = location.with_param(GLOBALS, settings_text(settings));
        }
        location
    }

    fn from_location(location: &Location) -> Option<Self> {
        let page = match location.segments() {
            [kind, id] if kind == PREVIEW && !id.is_empty() => Page::Preview(id.as_str().into()),
            [kind, title @ ..] if kind == DOCS && !title.is_empty() => {
                Page::Docs(title.join("/").into())
            }
            _ => return None,
        };
        let args = location.param(ARGS).map(parse_args);
        let settings = location.param(GLOBALS).map(parse_settings);
        let link = (args.is_some() || settings.is_some()).then(|| Link {
            args: args.unwrap_or_default(),
            settings,
        });
        Some(Self {
            page,
            chrome: location.param(CHROME) != Some("0"),
            link,
        })
    }
}

/// `args` as a link carries them: `name:value`, each value in its text form, joined by `;`.
pub(crate) fn args_text(args: &[(String, ArgValue)]) -> String {
    args.iter()
        .map(|(name, value)| format!("{name}:{value}"))
        .collect::<Vec<_>>()
        .join(";")
}

/// The args [`args_text`] wrote. A pair whose value is no arg value is left out.
pub(crate) fn parse_args(text: &str) -> Vec<(String, ArgValue)> {
    split_outside_quotes(text)
        .into_iter()
        .filter_map(|pair| {
            let (name, value) = pair.split_once(':')?;
            let name = name.trim();
            (!name.is_empty()).then_some(())?;
            Some((name.to_string(), value.trim().parse().ok()?))
        })
        .collect()
}

/// `text` cut at each `;` that is not inside a quoted text value.
fn split_outside_quotes(text: &str) -> Vec<&str> {
    let mut pieces = Vec::new();
    let (mut start, mut quoted, mut escaped) = (0, false, false);
    for (at, c) in text.char_indices() {
        match c {
            _ if escaped => escaped = false,
            '\\' if quoted => escaped = true,
            '"' => quoted = !quoted,
            ';' if !quoted => {
                pieces.push(&text[start..at]);
                start = at + 1;
            }
            _ => {}
        }
    }
    pieces.push(&text[start..]);
    pieces.retain(|piece| !piece.trim().is_empty());
    pieces
}

/// The settings that differ from the defaults, as `key:value` words joined by `;`, a switch that is on as its bare name.
pub(crate) fn settings_text(settings: &CanvasSettings) -> String {
    let mut words: Vec<String> = Vec::new();
    if let Some(mode) = &settings.mode {
        words.push(format!("mode:{mode}"));
    }
    if let Some(locale) = &settings.locale {
        words.push(format!("locale:{locale}"));
    }
    if let Some(direction) = settings.direction {
        words.push(format!("dir:{}", direction_word(direction)));
    }
    if let Some(size) = settings.control_size {
        words.push(format!("control_size:{}", control_size_word(size)));
    }
    if let Some(background) = background_word(settings.background) {
        words.push(format!("bg:{background}"));
    }
    if let Some(size) = size_word(&settings.size) {
        words.push(format!("size:{size}"));
    }
    if settings.rotated {
        words.push("rotated".to_string());
    }
    if let Zoom::Percent(percent) = settings.zoom {
        words.push(format!("zoom:{percent}"));
    }
    if settings.reduced_motion {
        words.push("reduced-motion".to_string());
    }
    if let Some(high) = settings.high_contrast {
        words.push(format!(
            "contrast:{}",
            if high { "more" } else { "standard" }
        ));
    }
    if settings.grid {
        words.push("grid".to_string());
    }
    if settings.rulers {
        words.push("rulers".to_string());
    }
    words.join(";")
}

/// The settings [`settings_text`] wrote, every other one at its default. A word this workshop does not know is passed over.
pub(crate) fn parse_settings(text: &str) -> CanvasSettings {
    let mut settings = CanvasSettings::default();
    for word in text
        .split(';')
        .map(str::trim)
        .filter(|word| !word.is_empty())
    {
        let (key, value) = word.split_once(':').unwrap_or((word, ""));
        match (key, value) {
            ("mode", mode) if !mode.is_empty() => settings.mode = Some(mode.to_string()),
            ("locale", locale) if !locale.is_empty() => settings.locale = Some(locale.to_string()),
            ("dir", direction) => settings.direction = parse_direction(direction),
            ("control_size", size) => settings.control_size = parse_control_size(size),
            ("bg", background) => settings.background = parse_background(background),
            ("size", size) => settings.size = parse_canvas_size(size).unwrap_or_default(),
            ("rotated", "") => settings.rotated = true,
            ("zoom", percent) => {
                settings.zoom = percent.parse().map_or(Zoom::Fit, Zoom::Percent);
            }
            ("reduced-motion", "") => settings.reduced_motion = true,
            ("contrast", "more") => settings.high_contrast = Some(true),
            ("contrast", "standard") => settings.high_contrast = Some(false),
            ("grid", "") => settings.grid = true,
            ("rulers", "") => settings.rulers = true,
            _ => {}
        }
    }
    settings
}

fn background_word(background: Background) -> Option<&'static str> {
    match background {
        Background::Preview => None,
        Background::Transparent => Some("transparent"),
        Background::Light => Some("light"),
        Background::Dark => Some("dark"),
    }
}

fn parse_background(word: &str) -> Background {
    match word {
        "transparent" => Background::Transparent,
        "light" => Background::Light,
        "dark" => Background::Dark,
        _ => Background::Preview,
    }
}

fn size_word(size: &CanvasSize) -> Option<String> {
    match size {
        CanvasSize::Preview => None,
        CanvasSize::Fill => Some("fill".to_string()),
        CanvasSize::Preset(id) => Some(format!("preset.{id}")),
        CanvasSize::Project(name) => Some(format!("project.{name}")),
        CanvasSize::Device(id) => Some(format!("device.{id}")),
        CanvasSize::Custom { width, height } => Some(format!("{width}x{height}")),
    }
}

fn parse_canvas_size(word: &str) -> Option<CanvasSize> {
    if word == "fill" {
        return Some(CanvasSize::Fill);
    }
    let named = |prefix: &str| {
        word.strip_prefix(prefix)
            .filter(|name| !name.is_empty())
            .map(str::to_string)
    };
    if let Some(id) = named("preset.") {
        return Some(CanvasSize::Preset(id));
    }
    if let Some(name) = named("project.") {
        return Some(CanvasSize::Project(name));
    }
    if let Some(id) = named("device.") {
        return Some(CanvasSize::Device(id));
    }
    let size = parse_size(word)?;
    Some(CanvasSize::Custom {
        width: size.width,
        height: size.height,
    })
}

#[cfg(test)]
#[path = "route_test.rs"]
mod tests;
