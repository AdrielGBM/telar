//! The browser's [`SystemPreferences`]: media queries for appearance, `navigator.languages` for locales and the root font size for the text scale, with the listeners that report changes.

use platform_core::{ColorScheme, SystemPreferences, TextScale};

use crate::dom;

const DARK: &str = "(prefers-color-scheme: dark)";
const LIGHT: &str = "(prefers-color-scheme: light)";
const REDUCED_MOTION: &str = "(prefers-reduced-motion: reduce)";
const MORE_CONTRAST: &str = "(prefers-contrast: more)";
const FORCED_COLORS: &str = "(forced-colors: active)";

/// Every query whose `change` event can move a field.
pub const WATCHED_QUERIES: [&str; 5] = [DARK, LIGHT, REDUCED_MOTION, MORE_CONTRAST, FORCED_COLORS];

/// What the page's media queries answered: `None` where the browser does not know the feature at all.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MediaAnswers {
    pub dark: Option<bool>,
    pub light: Option<bool>,
    pub reduced_motion: Option<bool>,
    pub more_contrast: Option<bool>,
    pub forced_colors: Option<bool>,
}

impl MediaAnswers {
    /// A forced-colours mode is the platform's own high-contrast theme (Windows' contrast themes reach a page this way), so it counts as asking for more contrast.
    pub fn into_preferences(
        self,
        locales: Vec<String>,
        text_scale: TextScale,
    ) -> SystemPreferences {
        let color_scheme = match (self.dark, self.light) {
            (Some(true), _) => Some(ColorScheme::Dark),
            (_, Some(true)) => Some(ColorScheme::Light),
            _ => None,
        };
        let high_contrast = match (self.more_contrast, self.forced_colors) {
            (None, None) => None,
            (more, forced) => Some(more.unwrap_or(false) || forced.unwrap_or(false)),
        };
        SystemPreferences {
            color_scheme,
            reduced_motion: self.reduced_motion,
            high_contrast,
            locales,
            text_scale,
        }
    }
}

/// Everything the page can see right now.
pub fn read() -> SystemPreferences {
    MediaAnswers {
        dark: matches(DARK),
        light: matches(LIGHT),
        reduced_motion: matches(REDUCED_MOTION),
        more_contrast: matches(MORE_CONTRAST),
        forced_colors: matches(FORCED_COLORS),
    }
    .into_preferences(languages(), root_text_scale())
}

// A browser that does not know a media feature parses the whole query to `not all`, which is how "unsupported" is told apart from "does not match".
fn matches(query: &str) -> Option<bool> {
    let list = media_query(query)?;
    (list.media() != "not all").then(|| list.matches())
}

/// The font size every browser gives the root element until its reader changes it, in CSS pixels.
const DEFAULT_ROOT_FONT_SIZE: f32 = 16.0;

/// The reader's text size setting: the root element's computed font size over the browser's default. Unscaled where the page cannot be asked.
fn root_text_scale() -> TextScale {
    let root = dom::document().document_element();
    let computed = root.and_then(|root| dom::window().get_computed_style(&root).ok().flatten());
    computed
        .and_then(|style| style.get_property_value("font-size").ok())
        .and_then(|size| size.strip_suffix("px")?.parse::<f32>().ok())
        .and_then(|size| TextScale::new(size / DEFAULT_ROOT_FONT_SIZE))
        .unwrap_or(TextScale::UNSCALED)
}

pub fn media_query(query: &str) -> Option<web_sys::MediaQueryList> {
    dom::window().match_media(query).ok().flatten()
}

fn languages() -> Vec<String> {
    let navigator = dom::window().navigator();
    let listed: Vec<String> = navigator
        .languages()
        .iter()
        .filter_map(|language| language.as_string())
        .filter(|language| !language.is_empty())
        .collect();
    if !listed.is_empty() {
        return listed;
    }
    navigator.language().into_iter().collect()
}
