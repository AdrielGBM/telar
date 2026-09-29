//! The low-level controls a face offers beyond family, weight and slant: where it sits on each of its variation axes, and which of its OpenType features are on.

use std::fmt::{self, Write};
use std::sync::Arc;

/// A four-letter OpenType tag: an axis (`wght`, `wdth`, `opsz`, `slnt`, or a face's own uppercase one) or a feature (`liga`, `tnum`, `ss01`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FontTag([u8; 4]);

impl FontTag {
    pub const fn new(tag: &[u8; 4]) -> Self {
        Self(*tag)
    }

    /// `text` as a tag, or `None` unless it is exactly four printable ASCII characters.
    pub fn parse(text: &str) -> Option<Self> {
        let bytes: [u8; 4] = text.as_bytes().try_into().ok()?;
        bytes
            .iter()
            .all(|byte| (0x20..=0x7e).contains(byte))
            .then_some(Self(bytes))
    }

    pub fn bytes(self) -> [u8; 4] {
        self.0
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.0).unwrap_or("????")
    }
}

impl fmt::Debug for FontTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FontTag({:?})", self.as_str())
    }
}

impl From<&str> for FontTag {
    /// Panics on a string that is not a tag; for literals. Runtime text goes through [`FontTag::parse`].
    fn from(text: &str) -> Self {
        Self::parse(text).unwrap_or_else(|| panic!("`{text}` is not a four-letter font tag"))
    }
}

/// Where the face sits on its variation axes, one value per tag, kept sorted by tag so two settings that say the same thing compare equal.
///
/// Like CSS `font-variation-settings`: a style that names any replaces what it inherited wholesale, and an axis the face lacks is ignored. `wght` here overrides `font_weight` on a face that has the axis.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FontVariations(Arc<[(FontTag, f32)]>);

impl FontVariations {
    pub fn new() -> Self {
        Self::default()
    }

    /// These settings with `tag` at `value`, replacing any value it had.
    pub fn with(self, tag: impl Into<FontTag>, value: f32) -> Self {
        let tag = tag.into();
        let mut axes: Vec<(FontTag, f32)> =
            self.0.iter().copied().filter(|(t, _)| *t != tag).collect();
        axes.push((tag, value));
        axes.sort_by_key(|(tag, _)| *tag);
        Self(axes.into())
    }

    pub fn get(&self, tag: impl Into<FontTag>) -> Option<f32> {
        let tag = tag.into();
        self.0
            .iter()
            .find(|(t, _)| *t == tag)
            .map(|(_, value)| *value)
    }

    pub fn iter(&self) -> impl Iterator<Item = (FontTag, f32)> + '_ {
        self.0.iter().copied()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The settings as CSS `font-variation-settings` writes them: `"wdth" 110, "wght" 650`, or `normal` when there are none.
    pub fn to_css(&self) -> String {
        css_list(self.0.iter().map(|(tag, value)| (*tag, format!("{value}"))))
    }

    /// These settings `t` of the way to `other`, axis by axis. An axis only one side names holds that side's value throughout, since the other gives no value to travel from.
    pub fn lerp(&self, other: &Self, t: f32) -> Self {
        let mut out = self.clone();
        for (tag, to) in other.iter() {
            let value = match self.get(tag) {
                Some(from) => from + (to - from) * t,
                None => to,
            };
            out = out.with(tag, value);
        }
        out
    }

    /// Settings built from `values` against the tags of `shape`, in its order: the inverse of [`values`](Self::values), for arithmetic that runs on plain numbers.
    pub fn with_values_of(shape: &Self, values: impl IntoIterator<Item = f32>) -> Self {
        Self(
            shape
                .0
                .iter()
                .zip(values)
                .map(|((tag, _), value)| (*tag, value))
                .collect(),
        )
    }

    /// Each axis's value, in tag order.
    pub fn values(&self) -> impl Iterator<Item = f32> + '_ {
        self.0.iter().map(|(_, value)| *value)
    }

    /// The settings' bits, for a cache key: equal settings hash equal.
    pub fn key(&self) -> u64 {
        self.0
            .iter()
            .fold(0xcbf2_9ce4_8422_2325, |hash, (tag, value)| {
                let mixed =
                    (hash ^ u64::from(u32::from_be_bytes(tag.0))).wrapping_mul(0x100_0000_01b3);
                (mixed ^ u64::from(value.to_bits())).wrapping_mul(0x100_0000_01b3)
            })
    }
}

/// Which OpenType features are on, one value per tag (`1` on, `0` off, larger to pick an alternate), kept sorted by tag.
///
/// Like CSS `font-feature-settings`: a style that names any replaces what it inherited, and a feature the face lacks is ignored.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct FontFeatures(Arc<[(FontTag, u32)]>);

impl FontFeatures {
    pub fn new() -> Self {
        Self::default()
    }

    /// These features with `tag` set to `value`, replacing any value it had.
    pub fn with(self, tag: impl Into<FontTag>, value: u32) -> Self {
        let tag = tag.into();
        let mut features: Vec<(FontTag, u32)> =
            self.0.iter().copied().filter(|(t, _)| *t != tag).collect();
        features.push((tag, value));
        features.sort_by_key(|(tag, _)| *tag);
        Self(features.into())
    }

    pub fn get(&self, tag: impl Into<FontTag>) -> Option<u32> {
        let tag = tag.into();
        self.0
            .iter()
            .find(|(t, _)| *t == tag)
            .map(|(_, value)| *value)
    }

    pub fn iter(&self) -> impl Iterator<Item = (FontTag, u32)> + '_ {
        self.0.iter().copied()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The features as CSS `font-feature-settings` writes them: `"liga" 0, "tnum" 1`, or `normal` when there are none.
    pub fn to_css(&self) -> String {
        css_list(self.0.iter().map(|(tag, value)| (*tag, value.to_string())))
    }

    /// The features' bits, for a cache key.
    pub fn key(&self) -> u64 {
        self.0
            .iter()
            .fold(0x8422_2325_cbf2_9ce4, |hash, (tag, value)| {
                let mixed =
                    (hash ^ u64::from(u32::from_be_bytes(tag.0))).wrapping_mul(0x100_0000_01b3);
                (mixed ^ u64::from(*value)).wrapping_mul(0x100_0000_01b3)
            })
    }
}

fn css_list(entries: impl Iterator<Item = (FontTag, String)>) -> String {
    let mut out = String::new();
    for (tag, value) in entries {
        if !out.is_empty() {
            out.push_str(", ");
        }
        let _ = write!(out, "\"{}\" {value}", tag.as_str().replace(['"', '\\'], ""));
    }
    if out.is_empty() {
        out.push_str("normal");
    }
    out
}

#[cfg(test)]
#[path = "font_settings_test.rs"]
mod tests;
