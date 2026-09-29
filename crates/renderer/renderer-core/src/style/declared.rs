//! [`Declared`]: a partial style — every field optional — which is what a node saying "bold from here down" needs, and what a byte range of a paragraph restyles itself with.

use geometry_core::Size;

use super::{
    FontFamily, FontFeatures, FontStyle, FontVariations, LineHeight, Paint, Raster, TextAlign,
    TextLength, TextShadow, TextStyle, TextWrap,
};

/// What one place says about the text style around it, each field `None` where it says nothing.
///
/// A *partial* style, and the only honest way to express "bold from here to there" or "this subtree is 11px": a whole `TextStyle` cannot, because every field it does not mean to change still holds a value that would overwrite one. Two callers want exactly this and would otherwise invent it twice — a span of a paragraph overriding the paragraph, and (next) a node overriding what it inherits. They are the same operation on the same data, so they are one type: a span is a cascade child whose extent is a byte range instead of a subtree.
///
/// Every field is an `Option` of a type that already carries its own absence where it has one — `LineHeight` rather than `Option<f32>`, `TextShadow` rather than `Option<Shadow>`. Without that the two would nest into an `Option<Option<T>>` whose layers mean entirely different things: "this node says nothing" and "this node says: none".
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Declared {
    pub font_family: Option<FontFamily>,
    pub font_size: Option<TextLength>,
    pub color: Option<Paint>,
    pub font_weight: Option<u16>,
    pub font_style: Option<FontStyle>,
    pub line_height: Option<LineHeight>,
    pub letter_spacing: Option<TextLength>,
    pub text_align: Option<TextAlign>,
    pub text_wrap: Option<TextWrap>,
    pub text_shadow: Option<TextShadow>,
    pub raster: Option<Raster>,
    pub font_variations: Option<FontVariations>,
    pub font_features: Option<FontFeatures>,
}

impl Declared {
    /// `base` with everything this declares applied over it, on a surface of `surface`: a size in `em` is of `base`'s, a spacing in `em` of the size this resolves to, and a fraction of the surface of `surface`.
    ///
    /// A renderer is handed spans already resolved against their surface (see [`on_surface`](Self::on_surface)), so it passes `Size::ZERO`: pixels and `em` are all it can meet.
    pub fn over(&self, base: &TextStyle, surface: Size) -> TextStyle {
        let mut out = base.clone();
        if let Some(family) = &self.font_family {
            out.font_family = family.clone();
        }
        if let Some(size) = self.font_size {
            out.font_size = size.resolve(base.font_size, surface);
        }
        if let Some(color) = self.color {
            out.color = color;
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
            out.letter_spacing = letter_spacing.resolve(out.font_size, surface);
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
        if let Some(font_variations) = &self.font_variations {
            out.font_variations = font_variations.clone();
        }
        if let Some(font_features) = &self.font_features {
            out.font_features = font_features.clone();
        }
        out
    }

    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Whether anything here is a fraction of the surface, and so has to be resolved again when the surface changes size.
    pub fn uses_surface(&self) -> bool {
        self.font_size.is_some_and(TextLength::is_surface_relative)
            || self
                .letter_spacing
                .is_some_and(TextLength::is_surface_relative)
    }

    /// This declaration with every fraction of the surface turned into the pixels it comes to on `surface`: what a span is before a renderer, which knows no surface, reads it.
    pub fn on_surface(&self, surface: Size) -> Self {
        Self {
            font_size: self.font_size.map(|size| size.on_surface(surface)),
            letter_spacing: self
                .letter_spacing
                .map(|spacing| spacing.on_surface(surface)),
            ..self.clone()
        }
    }

    pub fn with_font_weight(mut self, font_weight: u16) -> Self {
        self.font_weight = Some(font_weight);
        self
    }

    pub fn with_font_style(mut self, font_style: FontStyle) -> Self {
        self.font_style = Some(font_style);
        self
    }

    pub fn with_color(mut self, color: impl Into<Paint>) -> Self {
        self.color = Some(color.into());
        self
    }

    pub fn with_font_family(mut self, family: impl Into<FontFamily>) -> Self {
        self.font_family = Some(family.into());
        self
    }

    pub fn with_font_size(mut self, font_size: impl Into<TextLength>) -> Self {
        self.font_size = Some(font_size.into());
        self
    }

    pub fn with_letter_spacing(mut self, letter_spacing: impl Into<TextLength>) -> Self {
        self.letter_spacing = Some(letter_spacing.into());
        self
    }

    pub fn with_raster(mut self, raster: Raster) -> Self {
        self.raster = Some(raster);
        self
    }

    pub fn with_line_height(mut self, times: f32) -> Self {
        self.line_height = Some(LineHeight::Times(times));
        self
    }

    pub fn with_text_align(mut self, text_align: TextAlign) -> Self {
        self.text_align = Some(text_align);
        self
    }

    pub fn with_text_wrap(mut self, text_wrap: TextWrap) -> Self {
        self.text_wrap = Some(text_wrap);
        self
    }

    pub fn with_font_variations(mut self, font_variations: FontVariations) -> Self {
        self.font_variations = Some(font_variations);
        self
    }

    pub fn with_font_features(mut self, font_features: FontFeatures) -> Self {
        self.font_features = Some(font_features);
        self
    }
}

/// A byte range of a paragraph that styles itself differently from the paragraph.
///
/// `range` indexes the paragraph's own text, so the text stays one string — which is what lets a clamp re-shape it with an ellipsis. Runs that each owned their slice could not be cut across.
#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub range: std::ops::Range<u32>,
    pub over: Declared,
    /// Where the range takes the reader when activated: a link inside a paragraph. Each target makes it one its own way, as it does for a box (see `Destination`).
    pub link: Option<semantics_core::Destination>,
}

impl Span {
    pub fn new(range: std::ops::Range<u32>, over: Declared) -> Self {
        Self {
            range,
            over,
            link: None,
        }
    }

    /// This range as a link to `destination`.
    pub fn linking_to(mut self, destination: semantics_core::Destination) -> Self {
        self.link = Some(destination);
        self
    }
}

/// The span in `spans` whose range holds byte `index` and that links somewhere, with its position among the spans.
pub fn link_at(spans: &[Span], index: usize) -> Option<(usize, &semantics_core::Destination)> {
    spans.iter().enumerate().find_map(|(at, span)| {
        let holds = (span.range.start as usize..span.range.end as usize).contains(&index);
        span.link.as_ref().filter(|_| holds).map(|link| (at, link))
    })
}
