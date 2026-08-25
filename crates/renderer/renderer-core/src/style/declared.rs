use super::{FontFamily, GlyphRaster, Paint, TextStyle};

/// What one place says about the text style around it, each field `None` where it says nothing.
///
/// A *partial* style, and the only honest way to express "bold from here to there" or "this subtree is 11px":
/// a whole `TextStyle` cannot, because every field it does not mean to change still holds a value that would
/// overwrite one. Two callers want exactly this and would otherwise invent it twice — a span of a paragraph
/// overriding the paragraph, and (next) a node overriding what it inherits. They are the same operation on
/// the same data, so they are one type: a span is a cascade child whose extent is a byte range instead of a
/// subtree.
///
/// Deliberately smaller than the set of properties that will eventually flow down a tree: the ones missing
/// (`line_height`, `shadow`) are already `Option` on `TextStyle`, and nesting that inside this one would make
/// `Option<Option<T>>`, where the two layers mean different things. They arrive when they carry their own
/// absence as a value.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Declared {
    pub font_family: Option<FontFamily>,
    pub font_size: Option<f32>,
    pub paint: Option<Paint>,
    pub weight: Option<u16>,
    pub italic: Option<bool>,
    pub letter_spacing: Option<f32>,
    pub raster: Option<GlyphRaster>,
}

impl Declared {
    /// `base` with everything this declares applied over it.
    pub fn over(&self, base: &TextStyle) -> TextStyle {
        let mut out = base.clone();
        if let Some(family) = &self.font_family {
            out.font_family = family.clone();
        }
        if let Some(size) = self.font_size {
            out.font_size = size;
        }
        if let Some(paint) = self.paint {
            out.paint = paint;
        }
        if let Some(weight) = self.weight {
            out.weight = weight;
        }
        if let Some(italic) = self.italic {
            out.italic = italic;
        }
        if let Some(letter_spacing) = self.letter_spacing {
            out.letter_spacing = letter_spacing;
        }
        if let Some(raster) = self.raster {
            out.raster = raster;
        }
        out
    }

    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    pub fn with_weight(mut self, weight: u16) -> Self {
        self.weight = Some(weight);
        self
    }

    pub fn with_italic(mut self, italic: bool) -> Self {
        self.italic = Some(italic);
        self
    }

    pub fn with_paint(mut self, paint: impl Into<Paint>) -> Self {
        self.paint = Some(paint.into());
        self
    }

    pub fn with_font_family(mut self, family: impl Into<FontFamily>) -> Self {
        self.font_family = Some(family.into());
        self
    }

    pub fn with_font_size(mut self, font_size: f32) -> Self {
        self.font_size = Some(font_size);
        self
    }

    pub fn with_letter_spacing(mut self, letter_spacing: f32) -> Self {
        self.letter_spacing = Some(letter_spacing);
        self
    }

    pub fn with_raster(mut self, raster: GlyphRaster) -> Self {
        self.raster = Some(raster);
        self
    }
}

/// A byte range of a paragraph that styles itself differently from the paragraph.
///
/// `range` indexes the paragraph's own text, so the text stays one string — which is what lets a clamp
/// re-shape it with an ellipsis. Runs that each owned their slice could not be cut across.
#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub range: std::ops::Range<u32>,
    pub over: Declared,
}

impl Span {
    pub fn new(range: std::ops::Range<u32>, over: Declared) -> Self {
        Self { range, over }
    }
}
