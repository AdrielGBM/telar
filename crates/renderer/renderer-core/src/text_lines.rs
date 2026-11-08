//! [`TextLine`]: a line of text as it is drawn, cut into the grapheme clusters a caret steps over, each where it sits and which way it runs.

use std::ops::Range;

use rustc_hash::FxHashMap;
use unicode_segmentation::{GraphemeCursor, UnicodeSegmentation};

use crate::CasedText;

/// One grapheme cluster of a laid-out line: the bytes it was written as, where it is drawn and which way it runs.
#[derive(Clone, Debug, PartialEq)]
pub struct TextCluster {
    pub bytes: Range<usize>,
    /// Its left edge, from the left of the box the text is laid out in.
    pub x: f32,
    pub width: f32,
    /// Its bidi embedding level: even reads left to right, odd right to left.
    pub level: u8,
}

impl TextCluster {
    pub fn is_rtl(&self) -> bool {
        self.level % 2 == 1
    }

    /// Where a caret standing before it, in reading order, is drawn.
    pub fn leading_x(&self) -> f32 {
        match self.is_rtl() {
            true => self.x + self.width,
            false => self.x,
        }
    }

    /// Where a caret standing after it, in reading order, is drawn.
    pub fn trailing_x(&self) -> f32 {
        match self.is_rtl() {
            true => self.x,
            false => self.x + self.width,
        }
    }
}

/// One visual line of a laid-out text.
#[derive(Clone, Debug, PartialEq)]
pub struct TextLine {
    /// The bytes it holds, without the `\n` that ends it.
    pub range: Range<usize>,
    /// Whether its paragraph reads right to left.
    pub rtl: bool,
    /// Where it begins on the side its paragraph is read from, which is where a caret on a line holding nothing stands.
    pub start_x: f32,
    /// Its clusters from left to right, as drawn.
    pub clusters: Vec<TextCluster>,
}

/// A glyph as a surface placed it: the bytes of the shown text it was shaped from, its left edge, its advance and its bidi level.
#[derive(Clone, Debug, PartialEq)]
pub struct PlacedGlyph {
    pub bytes: Range<usize>,
    pub x: f32,
    pub width: f32,
    pub level: u8,
}

impl TextLine {
    /// The line holding `shown` of `cased`'s text, from the glyphs a surface placed on it, in bytes of `written`, the text `cased` was cased from.
    ///
    /// A glyph covering several clusters, a ligature, shares its advance between them evenly; the glyphs one cluster is drawn with, a letter and its marks, merge into one.
    pub fn from_glyphs(
        cased: &CasedText<'_>,
        written: &str,
        shown: Range<usize>,
        rtl: bool,
        start_x: f32,
        glyphs: impl IntoIterator<Item = PlacedGlyph>,
    ) -> Self {
        let text: &str = &cased.text;
        let mut clusters: Vec<TextCluster> = Vec::new();
        let mut by_start: FxHashMap<usize, usize> = FxHashMap::default();
        for glyph in glyphs {
            let start = grapheme_floor(text, glyph.bytes.start);
            let end = grapheme_ceil(text, glyph.bytes.end.max(glyph.bytes.start));
            let parts: Vec<Range<usize>> = text[start..end]
                .grapheme_indices(true)
                .map(|(at, part)| start + at..start + at + part.len())
                .collect();
            let share = glyph.width / parts.len().max(1) as f32;
            let last = parts.len().saturating_sub(1);
            for (index, part) in parts.into_iter().enumerate() {
                let slot = match glyph.level % 2 == 1 {
                    true => last - index,
                    false => index,
                };
                let cluster = TextCluster {
                    bytes: written_range(cased, written, part),
                    x: glyph.x + slot as f32 * share,
                    width: share,
                    level: glyph.level,
                };
                merge(&mut clusters, &mut by_start, cluster);
            }
        }
        clusters.sort_by(|a, b| a.x.total_cmp(&b.x));
        Self {
            range: written_range(cased, written, shown),
            rtl,
            start_x,
            clusters,
        }
    }

    /// The line holding `shown` of `cased`'s text drawn left to right in the order it is written, as a surface that does not reorder text draws it: each grapheme cluster as wide as `advance` says, the first at `start_x`.
    pub fn left_to_right(
        cased: &CasedText<'_>,
        written: &str,
        shown: Range<usize>,
        start_x: f32,
        mut advance: impl FnMut(&str) -> f32,
    ) -> Self {
        let mut x = start_x;
        let glyphs: Vec<PlacedGlyph> = cased.text[shown.clone()]
            .grapheme_indices(true)
            .map(|(at, part)| {
                let width = advance(part);
                let start = shown.start + at;
                let glyph = PlacedGlyph {
                    bytes: start..start + part.len(),
                    x,
                    width,
                    level: 0,
                };
                x += width;
                glyph
            })
            .collect();
        Self::from_glyphs(cased, written, shown, false, start_x, glyphs)
    }
}

fn merge(
    clusters: &mut Vec<TextCluster>,
    by_start: &mut FxHashMap<usize, usize>,
    cluster: TextCluster,
) {
    if cluster.bytes.is_empty() {
        return;
    }
    match by_start.get(&cluster.bytes.start) {
        Some(&index) => {
            let held = &mut clusters[index];
            let left = held.x.min(cluster.x);
            let right = (held.x + held.width).max(cluster.x + cluster.width);
            held.x = left;
            held.width = right - left;
            held.bytes.end = held.bytes.end.max(cluster.bytes.end);
        }
        None => {
            by_start.insert(cluster.bytes.start, clusters.len());
            clusters.push(cluster);
        }
    }
}

/// `shown` of the cased text, in whole grapheme clusters of `written`; a non-empty stretch keeps at least the cluster it starts in, since casing can grow a letter into more bytes than it was written in.
fn written_range(cased: &CasedText<'_>, written: &str, shown: Range<usize>) -> Range<usize> {
    let start = grapheme_floor(written, cased.source_index(shown.start));
    if shown.is_empty() {
        return start..start;
    }
    let end = grapheme_ceil(written, cased.source_index(shown.end));
    let at_least = grapheme_ceil(written, (start + 1).min(written.len()));
    start..end.max(at_least)
}

/// The grapheme cluster boundary at or before byte `at` of `text`.
fn grapheme_floor(text: &str, at: usize) -> usize {
    let mut at = at.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    let mut cursor = GraphemeCursor::new(at, text.len(), true);
    match cursor.is_boundary(text, 0) {
        Ok(true) => at,
        _ => cursor.prev_boundary(text, 0).ok().flatten().unwrap_or(0),
    }
}

/// The grapheme cluster boundary at or after byte `at` of `text`.
fn grapheme_ceil(text: &str, at: usize) -> usize {
    let floor = grapheme_floor(text, at);
    if floor >= at.min(text.len()) {
        return floor;
    }
    let mut cursor = GraphemeCursor::new(floor, text.len(), true);
    cursor
        .next_boundary(text, 0)
        .ok()
        .flatten()
        .unwrap_or(text.len())
}

#[cfg(test)]
#[path = "text_lines.test.rs"]
mod tests;
