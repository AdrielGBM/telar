//! [`WorkshopRoute`]: where the workshop is, as an address carries it — `/preview/<id>`, `/matrix/<id>` or `/docs/<package>/<title>` — with what is set on it there, so reloading the address, or opening a copy of it, comes back to the same place.
//!
//! - `?args=name:value;…`: the selected preview's args that differ from their defaults, each value in [`ArgValue`]'s text form.
//! - `?axes=mode;dir;arg.size;…`: on a matrix page, the axes picked in place of the preview's own matrix, present, even empty, whenever the view shows them; `declared.mode` is an axis that takes only the values the preview's own matrix gives it.
//! - `?cell=<axis>=<value>--…`: on a matrix page, the focused cell, by its id past the preview's.
//! - `?globals=key:value;…`: the canvas toolbar's settings, at their defaults adding nothing, which only a copied link carries: they are kept between runs where the workshop keeps its state, and the address the workshop reports as it moves leaves them out.
//! - `?chrome=0`: the canvas alone.
//!
//! What an address leaves out is left as it is when the workshop opens it, so a bare `/preview/<id>` moves to a preview without undoing what was set on it.

use std::sync::Arc;

use telar::preview::{
    ArgValue, control_size_word, direction_word, parse_control_size, parse_direction, parse_size,
};
use telar::{Location, Route};
use telar_project::protocol::title_path;

use crate::matrix::{CustomAxis, PickedAxis};
use crate::settings::{Background, CanvasSettings, CanvasSize, ModeChoice, Zoom};

const PREVIEW: &str = "preview";
const MATRIX: &str = "matrix";
const DOCS: &str = "docs";
const ARGS: &str = "args";
const AXES: &str = "axes";
const CELL: &str = "cell";
const GLOBALS: &str = "globals";
const CHROME: &str = "chrome";
const DECLARED: &str = "declared.";

/// Which page the workshop shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Page {
    /// A preview on its canvas, by id.
    Preview(Arc<str>),
    /// A preview's matrix, by the preview's id.
    Matrix(Arc<str>),
    /// A component's docs, by the package and title its previews are listed under, as [`docs_key`](telar_project::protocol::docs_key) names them.
    Docs(Arc<str>),
}

/// Where the workshop is: a page, whether its chrome shows, and what is set on it there.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WorkshopRoute {
    pub(crate) page: Page,
    pub(crate) chrome: bool,
    /// The selected preview's args that differ from their defaults, by name.
    pub(crate) args: Vec<(String, ArgValue)>,
    /// The axes the Matrix view varies in place of the preview's own matrix, while it does. Only a matrix page has any.
    pub(crate) axes: Option<Vec<PickedAxis>>,
    /// The focused matrix cell, by its whole id. Only a matrix page has one.
    pub(crate) cell: Option<String>,
    /// The canvas toolbar's settings, whole. Only a copied link carries them.
    pub(crate) settings: Option<CanvasSettings>,
}

impl WorkshopRoute {
    /// `page`, with nothing set on it.
    pub(crate) fn new(page: Page, chrome: bool) -> Self {
        Self {
            page,
            chrome,
            args: Vec::new(),
            axes: None,
            cell: None,
            settings: None,
        }
    }
}

impl Route for WorkshopRoute {
    fn to_location(&self) -> Location {
        let mut location = match &self.page {
            Page::Preview(id) => Location::from_segments([PREVIEW, id.as_ref()]),
            Page::Matrix(id) => Location::from_segments([MATRIX, id.as_ref()]),
            Page::Docs(key) => {
                Location::from_segments(std::iter::once(DOCS).chain(title_path(key)))
            }
        };
        if !self.chrome {
            location = location.with_param(CHROME, "0");
        }
        if !self.args.is_empty() {
            location = location.with_param(ARGS, args_text(&self.args));
        }
        if let Page::Matrix(id) = &self.page {
            if let Some(axes) = &self.axes {
                location = location.with_param(AXES, axes_text(axes));
            }
            if let Some(cell) = self.cell.as_deref().and_then(|cell| cell_past(id, cell)) {
                location = location.with_param(CELL, cell);
            }
        }
        if let Some(settings) = self
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
            [kind, id] if kind == MATRIX && !id.is_empty() => Page::Matrix(id.as_str().into()),
            [kind, key @ ..] if kind == DOCS && !key.is_empty() => Page::Docs(key.join("/").into()),
            _ => return None,
        };
        let (axes, cell) = match &page {
            Page::Matrix(id) => (
                location.param(AXES).map(parse_axes),
                location
                    .param(CELL)
                    .filter(|cell| !cell.is_empty())
                    .map(|cell| format!("{id}--{cell}")),
            ),
            Page::Preview(_) | Page::Docs(_) => (None, None),
        };
        Some(Self {
            chrome: location.param(CHROME) != Some("0"),
            args: location.param(ARGS).map(parse_args).unwrap_or_default(),
            axes,
            cell,
            settings: location.param(GLOBALS).map(parse_settings),
            page,
        })
    }
}

/// What of the cell id `cell` follows the preview `id`'s, or `None` for a cell of another preview.
pub(crate) fn cell_past<'a>(id: &str, cell: &'a str) -> Option<&'a str> {
    cell.strip_prefix(id)?
        .strip_prefix("--")
        .filter(|rest| !rest.is_empty())
}

/// `axes` as an address carries them: one word each, joined by `;`, an arg as `arg.<name>`, and one the preview's own matrix narrows prefixed with `declared.`.
pub(crate) fn axes_text(axes: &[PickedAxis]) -> String {
    axes.iter()
        .map(|picked| {
            let word = match &picked.axis {
                CustomAxis::Mode => "mode".to_string(),
                CustomAxis::Locale => "locale".to_string(),
                CustomAxis::Direction => "dir".to_string(),
                CustomAxis::ControlSize => "control_size".to_string(),
                CustomAxis::Viewport => "viewport".to_string(),
                CustomAxis::Arg(name) => format!("arg.{name}"),
            };
            match picked.declared {
                true => format!("{DECLARED}{word}"),
                false => word,
            }
        })
        .collect::<Vec<_>>()
        .join(";")
}

/// The axes [`axes_text`] wrote. A word this workshop does not know is passed over.
pub(crate) fn parse_axes(text: &str) -> Vec<PickedAxis> {
    text.split(';')
        .map(str::trim)
        .filter_map(|word| {
            let (declared, word) = match word.strip_prefix(DECLARED) {
                Some(rest) => (true, rest),
                None => (false, word),
            };
            let axis = parse_axis(word)?;
            Some(PickedAxis { axis, declared })
        })
        .collect()
}

fn parse_axis(word: &str) -> Option<CustomAxis> {
    match word {
        "mode" => Some(CustomAxis::Mode),
        "locale" => Some(CustomAxis::Locale),
        "dir" => Some(CustomAxis::Direction),
        "control_size" => Some(CustomAxis::ControlSize),
        "viewport" => Some(CustomAxis::Viewport),
        _ => word
            .strip_prefix("arg.")
            .filter(|name| !name.is_empty())
            .map(|name| CustomAxis::Arg(name.to_string())),
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
    if settings.mode != ModeChoice::System {
        words.push(format!("mode:{}", settings.mode.word()));
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
            ("mode", mode) => settings.mode = ModeChoice::parse(mode),
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
