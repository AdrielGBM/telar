//! What `cargo telar build --prerender` and an app built to prerender itself say to each other, and the state a prerendered page carries.
//!
//! The packager starts the app's own binary once per page with [`PRERENDER_ENV`] holding a [`PrerenderRequest`]; the app runs headless at that address, writes a [`PrerenderedPage`] where the request says, and exits. One process per page, because a page is what one browser tab loads: every page is built from nothing, the way the client that adopts it will be.
//!
//! [`PrerenderState`] is the half the client reads back. It goes into the page as `<script type="application/json" id="telar-state">`, and holds every input the page was built from that the client could not otherwise know to be the same.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// The variable the request travels in.
pub const PRERENDER_ENV: &str = "TELAR_PRERENDER";

/// The `id` of the script element a prerendered page carries its [`PrerenderState`] in.
pub const STATE_ELEMENT_ID: &str = "telar-state";

/// The version of [`PrerenderState`] this crate writes. A client reading a state of another version builds from nothing instead of adopting the page.
pub const STATE_VERSION: u32 = 1;

/// The surface a page is built for when the project names none in `[telar.web.prerender]`.
pub const DEFAULT_SURFACE: (u32, u32) = (1280, 800);

/// One page to build.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct PrerenderRequest {
    /// Where the [`PrerenderedPage`] is written, as JSON.
    pub out: PathBuf,
    /// The place the page shows.
    pub page: PageRequest,
    /// The surface the page is laid out on, in CSS pixels.
    pub surface: Surface,
    /// What the reader is assumed to prefer.
    pub preferences: Preferences,
    /// The path the site is served under, with a leading and a closing `/`: what the page's links and the build's files are written under.
    #[serde(default = "root_path")]
    pub base: String,
}

fn root_path() -> String {
    "/".to_string()
}

/// Which page a request is for.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PageRequest {
    /// The page at `location`. A location naming no locale opens in whichever one the app settles on, as it does in a browser.
    At { location: PageLocation },
    /// What the app shows at an address it has no page for.
    NotFound,
}

/// A location as a page is addressed by: its path, and the locale it is shown in. Nothing a static file cannot stand for: no query, no anchor.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct PageLocation {
    pub segments: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
}

impl PageLocation {
    pub fn root() -> Self {
        Self::default()
    }

    /// The same page in `locale`.
    pub fn in_locale(&self, locale: &str) -> Self {
        Self {
            segments: self.segments.clone(),
            locale: Some(locale.to_string()),
        }
    }

    /// Where the page's file goes under the site's root: its locale, then its segments, then `index.html`, which every static host serves for the directory's address with or without its closing `/`. `None` for a segment no file system path can hold.
    pub fn file(&self) -> Option<PathBuf> {
        let mut path = PathBuf::new();
        for segment in self.locale.iter().chain(&self.segments) {
            let plain = !segment.is_empty()
                && segment != "."
                && segment != ".."
                && !segment.contains(['/', '\\', '\0']);
            if !plain {
                return None;
            }
            path.push(segment);
        }
        path.push("index.html");
        Some(path)
    }

    /// How many directories below the site's root the page's file is.
    pub fn depth(&self) -> usize {
        self.segments.len() + usize::from(self.locale.is_some())
    }
}

/// The size of the surface a page is laid out on.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Surface {
    pub width: u32,
    pub height: u32,
}

impl Default for Surface {
    fn default() -> Self {
        Self {
            width: DEFAULT_SURFACE.0,
            height: DEFAULT_SURFACE.1,
        }
    }
}

/// The system preferences a page is built under: the ones a reader nobody has asked yet is assumed to have. `None` is a preference left unknown, which is what a browser reports before the app has read it.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub struct Preferences {
    /// `"light"` or `"dark"`.
    #[serde(default)]
    pub color_scheme: Option<String>,
    #[serde(default)]
    pub reduced_motion: Option<bool>,
    #[serde(default)]
    pub high_contrast: Option<bool>,
    /// BCP 47 tags, most preferred first.
    #[serde(default)]
    pub locales: Vec<String>,
}

/// One page, as the app built it.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct PrerenderedPage {
    /// The language the page is in, when the app set one.
    pub lang: Option<String>,
    /// The surface's title as the platform would show it: the app's, or the page's within it.
    pub title: String,
    /// The attributes the element the app fills carries.
    pub host_attributes: Vec<(String, String)>,
    /// What the page's `<head>` needs for the content to look as it will once the app runs.
    pub head: String,
    /// The content of the element the app fills.
    pub markup: String,
    /// Every input the page was built from, for the client to start from the same ones.
    pub state: PrerenderState,
    /// Every page the app declared while it ran at this one (see `Route::pages`), with no locale.
    pub pages: Vec<PageLocation>,
    /// The locales its address carries, as `follow_location_locale` bound them.
    pub locales: Vec<String>,
    /// The locale an address falls back to when nothing chooses another, as `follow_location_locale` bound it.
    #[serde(default)]
    pub base_locale: Option<String>,
    /// Whether the last two frames were the same, which is when the page is what the app shows rather than a moment on the way there.
    pub settled: bool,
}

/// What a prerendered page was built from: the inputs a client starts from to build the same tree, in the same order, and so the same elements with the same ids.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct PrerenderState {
    /// [`STATE_VERSION`] when it was written.
    pub version: u32,
    /// The address the page was built at, as the app writes it with no base path (`/es/projects`); `None` for the page an address with no page of its own is served.
    pub location: Option<String>,
    /// The locale the app was in.
    pub locale: Option<String>,
    /// What the reader was assumed to prefer.
    pub preferences: Preferences,
    /// The surface the page was laid out on.
    pub surface: Surface,
    /// The value of every keyed signal the tree read, as JSON by key: the `.rsx` signals of a build that keys them, each `hot_signal`, and the theme's mode and scheme preference.
    pub signals: BTreeMap<String, String>,
}

#[cfg(test)]
#[path = "prerender_test.rs"]
mod tests;
