//! Ruby: a reading set above the span of a paragraph it reads, how a paragraph sizes its readings, and the two ways a target lays a paragraph with readings out.

use std::borrow::Cow;
use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

use super::{CasedText, Span, span_segments};

/// Glues the grapheme clusters of a base together: no line break is allowed on either side of it, and it is invisible and takes no room.
pub const WORD_JOINER: char = '\u{2060}';

/// How a paragraph sets the readings its spans carry ([`Span::ruby`]), as fractions of the size of the base each one is set over.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RubyStyle {
    /// The size a reading is set in, and the height of the band above its base it takes.
    pub size: f32,
    /// The smallest a reading wider than its base shrinks to before its letters close up.
    pub min_size: f32,
}

impl Default for RubyStyle {
    fn default() -> Self {
        Self {
            size: 0.5,
            min_size: 0.46,
        }
    }
}

/// How one reading is set over its base: what [`RubyStyle::fit`] works out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RubyFit {
    /// The size the reading is set in.
    pub size: f32,
    /// How much closer each letter of the reading moves to the one before it.
    pub tightening: f32,
    /// How wide the reading comes out, set at `size` and tightened.
    pub width: f32,
    /// How much wider than its own glyphs the base is laid out, to hold a reading that could not be made to fit it.
    pub widening: f32,
}

impl RubyStyle {
    /// The most each letter of a reading closes up on the one before it, as a fraction of its size, before its base widens instead.
    pub const TIGHTEST: f32 = 0.1;

    /// The size a reading over a base of `base_size` is set in before it is fitted to its base.
    pub fn nominal(&self, base_size: f32) -> f32 {
        self.size * base_size
    }

    /// Fits a reading `width` wide at its [`nominal`](Self::nominal) size, with `letters` grapheme clusters, over a base `base_width` wide set at `base_size`: a reading that fits is set at its size, centred; one wider than its base shrinks towards [`min_size`](Self::min_size), then closes its letters up by at most [`TIGHTEST`](Self::TIGHTEST) of its size each, and only what is left after both widens the base.
    pub fn fit(&self, base_size: f32, base_width: f32, width: f32, letters: usize) -> RubyFit {
        let nominal = self.nominal(base_size);
        let set = |size: f32, tightening: f32, width: f32| RubyFit {
            size,
            tightening,
            width,
            widening: (width - base_width).max(0.0),
        };
        if width <= base_width || width <= 0.0 || nominal <= 0.0 {
            return set(nominal, 0.0, width);
        }
        let smallest = (self.min_size * base_size).min(nominal);
        let size = (nominal * base_width / width).max(smallest);
        let shrunk = width * size / nominal;
        let gaps = letters.saturating_sub(1);
        if shrunk <= base_width || gaps == 0 {
            return set(size, 0.0, shrunk);
        }
        let tightening = ((shrunk - base_width) / gaps as f32).min(Self::TIGHTEST * size);
        set(size, tightening, shrunk - tightening * gaps as f32)
    }
}

/// `base` with `reading` written after it in parentheses, `漢字(かんじ)`: how a reading is shown where it cannot be set above its base, and how it is read aloud.
pub fn ruby_inline(base: &str, reading: &str) -> String {
    format!("{base}({reading})")
}

impl<'a> CasedText<'a> {
    /// `text` and `spans` exactly as written, for a caller that reads a paragraph rather than drawing it, such as a screen reader.
    pub fn as_written(text: &'a str, spans: Option<&'a [Span]>) -> Self {
        CasedText {
            text: Cow::Borrowed(text),
            spans: spans.map(Cow::Borrowed),
            pieces: Vec::new(),
        }
    }

    /// This paragraph with every ruby base glued into one word, a [`WORD_JOINER`] between each two of its grapheme clusters, so a target that breaks lines where Unicode allows never breaks one inside a base. The readings stay on their spans, for the target to set above them.
    pub fn glued(self) -> CasedText<'a> {
        self.rewrite_bases(|shown, base, _, out| {
            for (offset, cluster) in shown[base.clone()].grapheme_indices(true) {
                let at = base.start + offset;
                if offset > 0 {
                    out.insert(WORD_JOINER.encode_utf8(&mut [0; 4]), at..at);
                }
                out.copy(shown, at..at + cluster.len());
            }
        })
    }

    /// This paragraph with every reading written after its base in parentheses, `漢字(かんじ)`, inside the base's span: what a target that cannot set a reading above its base shows, and what a screen reader reads. A press on the reading answers with its base.
    pub fn with_readings_inline(self) -> CasedText<'a> {
        let inline = self.rewrite_bases(|shown, base, reading, out| {
            let written = ruby_inline(&shown[base.clone()], reading);
            out.copy(shown, base.clone());
            out.insert(&written[base.len()..], base);
        });
        match inline.spans {
            Some(Cow::Owned(mut spans)) => {
                for span in &mut spans {
                    span.ruby = None;
                }
                CasedText {
                    spans: Some(Cow::Owned(spans)),
                    ..inline
                }
            }
            _ => inline,
        }
    }

    /// The paragraph with each ruby base written by `base`, everything else copied, its spans moved onto what was written, and the way back composed with this one's. Borrowed through untouched when no span carries a reading.
    fn rewrite_bases(
        self,
        mut base: impl FnMut(&str, Range<usize>, &str, &mut Rewritten),
    ) -> CasedText<'a> {
        let Some(spans) = self.spans.as_deref() else {
            return self;
        };
        if spans.iter().all(|span| span.ruby.is_none()) {
            return self;
        }
        let shown: &str = &self.text;
        let mut out = Rewritten::default();
        let mut moved = Vec::with_capacity(spans.len());
        for segment in span_segments(shown, spans) {
            let start = out.text.len();
            match segment.span.and_then(|(_, span)| span.ruby.as_deref()) {
                Some(reading) => base(shown, segment.range, reading, &mut out),
                None => out.copy(shown, segment.range),
            }
            if let Some((_, span)) = segment.span {
                moved.push(Span {
                    range: start as u32..out.text.len() as u32,
                    ..span.clone()
                });
            }
        }
        let pieces = out
            .pieces
            .into_iter()
            .map(|(rewritten, shown)| (rewritten, self.written(shown)))
            .collect();
        CasedText {
            text: Cow::Owned(out.text),
            spans: Some(Cow::Owned(moved)),
            pieces,
        }
    }
}

/// A paragraph being written out again, with each stretch of it and the stretch of the paragraph it stands for.
#[derive(Default)]
struct Rewritten {
    text: String,
    pieces: Vec<(Range<usize>, Range<usize>)>,
}

impl Rewritten {
    fn copy(&mut self, shown: &str, range: Range<usize>) {
        let start = self.text.len();
        self.text.push_str(&shown[range.clone()]);
        self.pieces.push((start..self.text.len(), range));
    }

    fn insert(&mut self, inserted: &str, stands_for: Range<usize>) {
        let start = self.text.len();
        self.text.push_str(inserted);
        self.pieces.push((start..self.text.len(), stands_for));
    }
}

#[cfg(test)]
#[path = "ruby.test.rs"]
mod tests;
