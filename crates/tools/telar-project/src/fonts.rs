//! `[[telar.fonts]]`: the faces a project ships, declared once for every target.
//!
//! One entry is one file, the way one `@font-face` rule is: a family with a regular and an italic file is two entries naming the same family. What each target does with it is its own business — the web packaging writes the `@font-face` and a preload, `telar::app!` embeds the bytes for a native shaper, a terminal ignores it — which is why the declaration says what the face *is* and nothing about how any one target loads it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// One face file and what it answers to.
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FontDeclaration {
    /// The family text asks for this face by. Authoritative, as `font-family` in an `@font-face` is: it need not match the family the file declares.
    pub family: String,
    /// The file, relative to the package root: `.ttf` or `.otf` for every target, `.woff`/`.woff2` for a document only.
    pub src: String,
    /// One weight, or `[min, max]` for a variable face. Defaults to the `wght` axis when one is declared, else 400.
    pub weight: Option<WeightDeclaration>,
    #[serde(default)]
    pub style: FontStyleDeclaration,
    /// Each variation axis the face offers, by tag: `{ wght = [100, 900], opsz = [14, 32] }`.
    #[serde(default)]
    pub axes: BTreeMap<String, [f32; 2]>,
    /// How a document shows text while the face is loading.
    #[serde(default)]
    pub display: FontDisplay,
    /// A factor a document scales the face's glyphs by, to match the metrics of the fallback it replaces. `1.05` is `size-adjust: 105%`.
    pub size_adjust: Option<f32>,
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(untagged)]
pub enum WeightDeclaration {
    Fixed(u16),
    Range([u16; 2]),
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum FontStyleDeclaration {
    #[default]
    Normal,
    Italic,
    Oblique,
}

impl FontStyleDeclaration {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Italic => "italic",
            Self::Oblique => "oblique",
        }
    }
}

/// The CSS `font-display` values. `swap` by default: text shows at once in the fallback and changes face when this one lands, which is what not waiting for the font means.
#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum FontDisplay {
    Auto,
    Block,
    #[default]
    Swap,
    Fallback,
    Optional,
}

impl FontDisplay {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Block => "block",
            Self::Swap => "swap",
            Self::Fallback => "fallback",
            Self::Optional => "optional",
        }
    }
}

/// The container a face file is in, read off its extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontFormat {
    TrueType,
    OpenType,
    Woff,
    Woff2,
}

impl FontFormat {
    fn of(src: &str) -> Option<Self> {
        let extension = Path::new(src).extension()?.to_str()?.to_ascii_lowercase();
        match extension.as_str() {
            "ttf" => Some(Self::TrueType),
            "otf" => Some(Self::OpenType),
            "woff" => Some(Self::Woff),
            "woff2" => Some(Self::Woff2),
            _ => None,
        }
    }

    /// What `format()` in an `@font-face` `src` names it.
    pub fn css_name(self) -> &'static str {
        match self {
            Self::TrueType => "truetype",
            Self::OpenType => "opentype",
            Self::Woff => "woff",
            Self::Woff2 => "woff2",
        }
    }

    /// Whether a glyph shaper can read it. WOFF is a compressed wrapper only a browser unpacks, so a face shipped only as WOFF reaches a document and nothing else.
    pub fn is_sfnt(self) -> bool {
        matches!(self, Self::TrueType | Self::OpenType)
    }
}

impl FontDeclaration {
    pub fn path(&self, package_root: &Path) -> PathBuf {
        package_root.join(&self.src)
    }

    pub fn format(&self) -> FontFormat {
        FontFormat::of(&self.src).expect("checked when the manifest was read")
    }

    /// The weights the face covers, as `(min, max)`.
    pub fn weight_range(&self) -> (u16, u16) {
        match self.weight {
            Some(WeightDeclaration::Fixed(weight)) => (weight, weight),
            Some(WeightDeclaration::Range([min, max])) => (min, max),
            None => self
                .axes
                .get("wght")
                .map(|[min, max]| (*min as u16, *max as u16))
                .unwrap_or((400, 400)),
        }
    }

    /// The widths the face covers as percentages, from its `wdth` axis.
    pub fn stretch_range(&self) -> Option<(f32, f32)> {
        self.axes.get("wdth").map(|[min, max]| (*min, *max))
    }

    /// Everything a well-formed file can still get wrong, as one message per mistake.
    pub fn problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        let at = format!("`[[telar.fonts]]` for {:?}", self.family);
        if self.family.trim().is_empty() {
            problems.push("a `[[telar.fonts]]` entry has an empty `family`".to_string());
        }
        if FontFormat::of(&self.src).is_none() {
            problems.push(format!(
                "{at}: `src = {:?}` is not a `.ttf`, `.otf`, `.woff` or `.woff2` file",
                self.src
            ));
        }
        let (min, max) = self.weight_range();
        if !(1..=1000).contains(&min) || !(1..=1000).contains(&max) || min > max {
            problems.push(format!(
                "{at}: the weight must be between 1 and 1000, low to high; it is {min}..{max}"
            ));
        }
        for (tag, [low, high]) in &self.axes {
            if tag.len() != 4 || !tag.bytes().all(|b| b.is_ascii_graphic()) {
                problems.push(format!(
                    "{at}: `{tag}` is not an axis tag, which is four letters such as `wght`"
                ));
            }
            if !low.is_finite() || !high.is_finite() || low > high {
                problems.push(format!(
                    "{at}: the axis `{tag}` must be `[min, max]`, low to high"
                ));
            }
        }
        if let Some(factor) = self.size_adjust
            && !(factor.is_finite() && factor > 0.0)
        {
            problems.push(format!(
                "{at}: `size_adjust` must be a positive factor, such as 1.05"
            ));
        }
        problems
    }

    /// The axis tag as the four bytes a font names it by. Only meaningful once [`problems`](Self::problems) found none.
    pub fn axis_tags(&self) -> impl Iterator<Item = ([u8; 4], f32, f32)> + '_ {
        self.axes.iter().filter_map(|(tag, [min, max])| {
            let tag: [u8; 4] = tag.as_bytes().try_into().ok()?;
            Some((tag, *min, *max))
        })
    }
}

#[cfg(test)]
#[path = "fonts_test.rs"]
mod tests;
