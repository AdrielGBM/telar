use super::{
    FontFamily, FontStyle, LineHeight, Paint, Raster, TextAlign, TextShadow, TextStyle, TextWrap,
};

/// What one place says about the text style around it, each field `None` where it says nothing.
///
/// A *partial* style, and the only honest way to express "bold from here to there" or "this subtree is 11px":
/// a whole `TextStyle` cannot, because every field it does not mean to change still holds a value that would
/// overwrite one. Two callers want exactly this and would otherwise invent it twice — a span of a paragraph
/// overriding the paragraph, and (next) a node overriding what it inherits. They are the same operation on
/// the same data, so they are one type: a span is a cascade child whose extent is a byte range instead of a
/// subtree.
///
/// Every field is an `Option` of a type that already carries its own absence where it has one — `LineHeight`
/// rather than `Option<f32>`, `TextShadow` rather than `Option<Shadow>`. Without that the two would nest into
/// an `Option<Option<T>>` whose layers mean entirely different things: "this node says nothing" and "this
/// node says: none".
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Declared {
    pub font_family: Option<FontFamily>,
    pub font_size: Option<f32>,
    pub paint: Option<Paint>,
    pub font_weight: Option<u16>,
    pub font_style: Option<FontStyle>,
    pub line_height: Option<LineHeight>,
    pub letter_spacing: Option<f32>,
    pub text_align: Option<TextAlign>,
    pub text_wrap: Option<TextWrap>,
    pub text_shadow: Option<TextShadow>,
    pub raster: Option<Raster>,
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
        if let Some(font_weight) = self.font_weight {
            out.font_weight = font_weight;
        }
        if let Some(font_style) = self.font_style {
            out.font_style = font_style;
        }
        if let Some(line_height) = self.line_height {
            out.line_height = line_height;
        }
        if let Some(letter_spacing) = self.letter_spacing {
            out.letter_spacing = letter_spacing;
        }
        if let Some(text_align) = self.text_align {
            out.text_align = text_align;
        }
        if let Some(text_wrap) = self.text_wrap {
            out.text_wrap = text_wrap;
        }
        if let Some(text_shadow) = self.text_shadow {
            out.text_shadow = text_shadow;
        }
        if let Some(raster) = self.raster {
            out.raster = raster;
        }
        out
    }

    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    pub fn with_font_weight(mut self, font_weight: u16) -> Self {
        self.font_weight = Some(font_weight);
        self
    }

    pub fn with_font_style(mut self, font_style: FontStyle) -> Self {
        self.font_style = Some(font_style);
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

    pub fn with_raster(mut self, raster: Raster) -> Self {
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
