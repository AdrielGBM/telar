//! The fonts a renderer is built with: the faces it loads, and the families to prefer.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::FontStyle;

/// Shared font configuration for both software and hardware renderers. Lets callers supply their own faces, a system fonts directory, and preferred sans-serif families without duplicating these fields across renderer-specific config structs.
#[derive(Clone, Default)]
pub struct FontConfig {
    pub faces: Vec<FontAsset>,
    pub system_fonts_dir: Option<PathBuf>,
    pub sans_serif_family_candidates: Vec<String>,
}

/// A face an application ships: where its bytes come from, and what it answers to.
///
/// The same concept on every target. A glyph shaper loads the bytes, a document hands them to the browser as an `@font-face`, and a terminal ignores the whole thing because a cell has no typeface to choose.
#[derive(Clone, Debug, PartialEq)]
pub struct FontAsset {
    pub source: FontSource,
    /// The family text asks for this face by. `None` keeps the family the file declares; `Some` makes this name authoritative, as `font-family` in an `@font-face` is.
    pub family: Option<Arc<str>>,
    pub weight: FontWeight,
    pub style: FontStyle,
    /// The variation axes the face offers, as declared. Carried so a style can later animate them; nothing reads them to pick a face.
    pub axes: Vec<FontAxis>,
}

impl FontAsset {
    fn new(source: FontSource) -> Self {
        Self {
            source,
            family: None,
            weight: FontWeight::default(),
            style: FontStyle::Normal,
            axes: Vec::new(),
        }
    }

    /// A face compiled into the binary, as `include_bytes!` gives it.
    pub fn embedded(bytes: &'static [u8]) -> Self {
        Self::new(FontSource::Static(bytes))
    }

    /// A face whose bytes arrived at run time.
    pub fn bytes(bytes: impl Into<Arc<[u8]>>) -> Self {
        Self::new(FontSource::Shared(bytes.into()))
    }

    /// A face read from disk when it is loaded. A relative path is resolved against the directory the executable is in, which is where a packaged application keeps the files it ships beside itself.
    pub fn file(path: impl Into<PathBuf>) -> Self {
        Self::new(FontSource::File(path.into()))
    }

    pub fn named(mut self, family: impl Into<Arc<str>>) -> Self {
        self.family = Some(family.into());
        self
    }

    pub fn with_weight(mut self, weight: FontWeight) -> Self {
        self.weight = weight;
        self
    }

    pub fn with_style(mut self, style: FontStyle) -> Self {
        self.style = style;
        self
    }

    pub fn with_axis(mut self, axis: FontAxis) -> Self {
        self.axes.push(axis);
        self
    }
}

/// Where a face's bytes come from.
#[derive(Clone, Debug)]
pub enum FontSource {
    Static(&'static [u8]),
    Shared(Arc<[u8]>),
    File(PathBuf),
}

impl FontSource {
    /// The path this source reads, resolved: absolute as given, relative against `executable_dir`.
    pub fn resolved_path(&self, executable_dir: Option<&Path>) -> Option<PathBuf> {
        let FontSource::File(path) = self else {
            return None;
        };
        match (path.is_relative(), executable_dir) {
            (true, Some(dir)) => Some(dir.join(path)),
            _ => Some(path.clone()),
        }
    }
}

// By identity first: a face is megabytes, and the common comparison is a configuration against the one it was cloned from.
impl PartialEq for FontSource {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Static(a), Self::Static(b)) => std::ptr::eq(*a, *b) || a == b,
            (Self::Shared(a), Self::Shared(b)) => Arc::ptr_eq(a, b) || a == b,
            (Self::File(a), Self::File(b)) => a == b,
            _ => false,
        }
    }
}

/// The weights a face covers: one for a static face, a range for a variable one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FontWeight {
    pub min: u16,
    pub max: u16,
}

impl FontWeight {
    pub const fn fixed(weight: u16) -> Self {
        Self {
            min: weight,
            max: weight,
        }
    }

    pub const fn range(min: u16, max: u16) -> Self {
        Self { min, max }
    }
}

impl Default for FontWeight {
    fn default() -> Self {
        Self::fixed(400)
    }
}

/// One variation axis a face offers: its four-letter tag and the range it covers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontAxis {
    pub tag: [u8; 4],
    pub min: f32,
    pub max: f32,
}

impl FontAxis {
    pub const fn new(tag: [u8; 4], min: f32, max: f32) -> Self {
        Self { tag, min, max }
    }
}

#[cfg(test)]
#[path = "font_config_test.rs"]
mod tests;
