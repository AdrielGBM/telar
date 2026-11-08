//! The names a packaged page and the app running in it agree on: what the page writes, the browser runtime looks up by the same constant.

use crate::protocol::{UnknownValue, parse_word};

/// The id of the element the page sets aside for the app to fill.
pub const HOST_ELEMENT_ID: &str = "telar-root";

/// The `name` of the meta tag that says where the build's files live, relative to the page.
pub const ASSETS_META_NAME: &str = "telar-assets";

/// What a font preload carries to name the family its face answers to.
pub const FONT_FAMILY_ATTRIBUTE: &str = "data-telar-family";

/// The page setting saying how the app draws, whose values are [`PageRenderer`]'s.
pub const RENDERER_SETTING: &str = "renderer";

/// How the page asks the app to draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PageRenderer {
    /// Pixels where the browser offers a GPU adapter, a document where it does not.
    #[default]
    Auto,
    /// Pixels on a canvas, through WebGPU.
    Canvas,
    /// Real elements, laid out by CSS.
    Dom,
}

impl PageRenderer {
    pub const ALL: [Self; 3] = [Self::Auto, Self::Canvas, Self::Dom];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Canvas => "canvas",
            Self::Dom => "dom",
        }
    }

    pub fn parse(word: &str) -> Result<Self, UnknownValue> {
        parse_word(&Self::ALL, Self::as_str, word)
    }
}

#[cfg(test)]
#[path = "page.test.rs"]
mod tests;
