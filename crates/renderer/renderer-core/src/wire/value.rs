//! How each value a draw command carries is spelled, field by field, with every enum's tags written out so the wire never follows declaration order. Geometry and semantics are spelled beside their own types, in `geometry-core` and `semantics-core`.

use semantics_core::wire::{put_link, take_link};
use wire_core::{Reader, Wire, WireError, Writer, wire_enum, wire_struct};

use crate::{
    BlendMode, Border, Clamp, Color, Dash, Declared, DecorationLength, DecorationLine,
    DecorationMetric, Element, ElementId, FillRule, FontFamily, FontFeatures, FontStyle, FontTag,
    FontVariations, Gradient, GradientKind, GradientStops, ImageFill, ImageSlice, LayerMask,
    LineCap, LineHeight, LineJoin, Linked, Paint, PathData, PathStyle, PathVerb, Picture, Raster,
    RectStyle, Shadow, Span, Stroke, TextAlign, TextCase, TextDecoration, TextLength, TextShadow,
    TextStyle, TextWrap,
};

wire_enum!(TextAlign {
    Start = 0,
    Center = 1,
    End = 2,
    Justify = 3,
});
wire_enum!(Raster {
    Smooth = 0,
    Pixel = 1
});
wire_enum!(FontStyle {
    Normal = 0,
    Italic = 1,
    Oblique = 2
});
wire_enum!(TextWrap {
    Wrap = 0,
    NoWrap = 1
});
wire_enum!(LineHeight {
    Natural = 0,
    Times(factor) = 1
});
wire_enum!(TextShadow {
    None = 0,
    Cast(shadow) = 1
});
wire_enum!(TextCase {
    AsWritten = 0,
    Upper = 1,
    Lower = 2,
    Capitalize = 3,
});
wire_enum!(TextLength {
    Px(value) = 0,
    Em(value) = 1,
    SurfaceWidth(value) = 2,
    SurfaceHeight(value) = 3,
    SurfaceMin(value) = 4,
    SurfaceMax(value) = 5,
});
wire_enum!(DecorationLine {
    None = 0,
    Underline = 1
});
wire_enum!(DecorationMetric {
    FromFont = 0,
    Px(px) = 1
});
wire_enum!(DecorationLength {
    FromFont = 0,
    Length(length) = 1
});
wire_enum!(FontFamily {
    SansSerif = 0,
    Serif = 1,
    Monospace = 2,
    SystemUi = 3,
    Cursive = 4,
    Fantasy = 5,
    Named(name) = 6,
    Stack(families) = 7,
});
wire_enum!(Paint {
    Solid(color) = 0,
    Gradient(gradient) = 1
});
wire_enum!(LineCap {
    Butt = 0,
    Round = 1,
    Square = 2
});
wire_enum!(LineJoin {
    Miter = 0,
    Round = 1,
    Bevel = 2
});
wire_enum!(FillRule {
    Winding = 0,
    EvenOdd = 1
});
wire_enum!(BlendMode {
    Normal = 0,
    Multiply = 1,
    Screen = 2,
    Overlay = 3,
    Darken = 4,
    Lighten = 5,
    ColorDodge = 6,
    ColorBurn = 7,
    HardLight = 8,
    SoftLight = 9,
    Difference = 10,
    Exclusion = 11,
    Hue = 12,
    Saturation = 13,
    Color = 14,
    Luminosity = 15,
    Plus = 16,
});
wire_enum!(LayerMask {
    None = 0,
    Source = 1,
    Apply = 2
});

wire_struct!(Shadow {
    offset_x,
    offset_y,
    blur_radius,
    spread,
    color
});
wire_struct!(Gradient { kind, stops });
wire_struct!(Border { paint, widths });
wire_struct!(RectStyle {
    fill,
    border,
    shadow,
    radius
});
wire_struct!(Stroke {
    paint,
    width,
    cap,
    join,
    dash
});
wire_struct!(PathStyle {
    fill,
    stroke,
    shadow,
    fill_rule
});
wire_struct!(TextDecoration {
    line,
    offset,
    thickness,
    color
});
wire_struct!(TextStyle {
    font_size,
    color,
    text_shadow,
    font_family,
    font_weight,
    font_style,
    text_align,
    clamp,
    line_height,
    letter_spacing,
    raster,
    text_wrap,
    font_variations,
    font_features,
    text_case,
    decoration,
    lang,
});
wire_struct!(Declared {
    font_family,
    font_size,
    color,
    font_weight,
    font_style,
    line_height,
    letter_spacing,
    text_align,
    text_wrap,
    text_shadow,
    raster,
    font_variations,
    font_features,
    text_case,
    decoration_line,
    decoration_offset,
    decoration_thickness,
    decoration_color,
});

impl Wire for Span {
    fn put(&self, w: &mut Writer) {
        self.range.put(w);
        self.over.put(w);
        put_link(w, self.link.as_ref());
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        Ok(Span {
            range: Wire::take(r)?,
            over: Wire::take(r)?,
            link: take_link(r)?,
        })
    }
}

wire_struct!(ImageSlice { insets, scale });

impl Wire for GradientKind {
    fn put(&self, w: &mut Writer) {
        match self {
            GradientKind::Linear { start, end } => {
                w.u8(0);
                start.put(w);
                end.put(w);
            }
            GradientKind::Radial { center, radius } => {
                w.u8(1);
                center.put(w);
                radius.put(w);
            }
        }
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        match r.u8()? {
            0 => Ok(GradientKind::Linear {
                start: Wire::take(r)?,
                end: Wire::take(r)?,
            }),
            1 => Ok(GradientKind::Radial {
                center: Wire::take(r)?,
                radius: Wire::take(r)?,
            }),
            tag => Err(WireError::Tag {
                of: "GradientKind",
                tag,
            }),
        }
    }
}

impl Wire for GradientStops {
    fn put(&self, w: &mut Writer) {
        w.len(self.active().len());
        for stop in self.active() {
            stop.position.put(w);
            stop.color.put(w);
        }
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        let len = r.len()?;
        if len > 8 {
            return Err(WireError::Invalid("more than 8 gradient stops"));
        }
        let stops: Vec<(f32, Color)> = (0..len)
            .map(|_| Ok((f32::take(r)?, Color::take(r)?)))
            .collect::<Result<_, WireError>>()?;
        Ok(GradientStops::new(&stops))
    }
}

impl Wire for Dash {
    fn put(&self, w: &mut Writer) {
        w.len(self.lengths().len());
        for length in self.lengths() {
            length.put(w);
        }
        self.offset().put(w);
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        let len = r.len()?;
        let lengths: Vec<f32> = (0..len).map(|_| f32::take(r)).collect::<Result<_, _>>()?;
        Dash::new(&lengths, f32::take(r)?).ok_or(WireError::Invalid("dash pattern"))
    }
}

impl Wire for Clamp {
    fn put(&self, w: &mut Writer) {
        match self {
            Clamp::None => w.u8(0),
            Clamp::Lines { max, ellipsis } => {
                w.u8(1);
                max.get().put(w);
                ellipsis.put(w);
            }
        }
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        match r.u8()? {
            0 => Ok(Clamp::None),
            1 => {
                let max = std::num::NonZeroU16::new(u16::take(r)?)
                    .ok_or(WireError::Invalid("a clamp to zero lines"))?;
                Ok(Clamp::Lines {
                    max,
                    ellipsis: bool::take(r)?,
                })
            }
            tag => Err(WireError::Tag { of: "Clamp", tag }),
        }
    }
}

impl Wire for FontTag {
    fn put(&self, w: &mut Writer) {
        w.raw(&self.bytes());
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        let bytes: [u8; 4] = r.raw(4)?.try_into().map_err(|_| WireError::Truncated)?;
        Ok(FontTag::new(&bytes))
    }
}

impl Wire for FontVariations {
    fn put(&self, w: &mut Writer) {
        w.len(self.iter().count());
        for (tag, value) in self.iter() {
            tag.put(w);
            value.put(w);
        }
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        let len = r.len()?;
        (0..len).try_fold(FontVariations::new(), |axes, _| {
            Ok(axes.with(FontTag::take(r)?, f32::take(r)?))
        })
    }
}

impl Wire for FontFeatures {
    fn put(&self, w: &mut Writer) {
        w.len(self.iter().count());
        for (tag, value) in self.iter() {
            tag.put(w);
            value.put(w);
        }
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        let len = r.len()?;
        (0..len).try_fold(FontFeatures::new(), |features, _| {
            Ok(features.with(FontTag::take(r)?, u32::take(r)?))
        })
    }
}

impl Wire for PathVerb {
    fn put(&self, w: &mut Writer) {
        match self {
            PathVerb::MoveTo(to) => {
                w.u8(0);
                to.put(w);
            }
            PathVerb::LineTo(to) => {
                w.u8(1);
                to.put(w);
            }
            PathVerb::QuadTo { ctrl, to } => {
                w.u8(2);
                ctrl.put(w);
                to.put(w);
            }
            PathVerb::CubicTo { ctrl1, ctrl2, to } => {
                w.u8(3);
                ctrl1.put(w);
                ctrl2.put(w);
                to.put(w);
            }
            PathVerb::Close => w.u8(4),
        }
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        match r.u8()? {
            0 => Ok(PathVerb::MoveTo(Wire::take(r)?)),
            1 => Ok(PathVerb::LineTo(Wire::take(r)?)),
            2 => Ok(PathVerb::QuadTo {
                ctrl: Wire::take(r)?,
                to: Wire::take(r)?,
            }),
            3 => Ok(PathVerb::CubicTo {
                ctrl1: Wire::take(r)?,
                ctrl2: Wire::take(r)?,
                to: Wire::take(r)?,
            }),
            4 => Ok(PathVerb::Close),
            tag => Err(WireError::Tag {
                of: "PathVerb",
                tag,
            }),
        }
    }
}

impl Wire for PathData {
    fn put(&self, w: &mut Writer) {
        w.len(self.verbs.len());
        for verb in &self.verbs {
            verb.put(w);
        }
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        let len = r.len()?;
        let verbs = (0..len)
            .map(|_| PathVerb::take(r))
            .collect::<Result<_, _>>()?;
        Ok(PathData::from_verbs(verbs))
    }
}

impl Wire for ImageFill {
    fn put(&self, w: &mut Writer) {
        match self {
            ImageFill::Stretch => w.u8(0),
            ImageFill::Tile { scale } => {
                w.u8(1);
                scale.put(w);
            }
            ImageFill::Slice(slice) => {
                w.u8(2);
                slice.put(w);
            }
        }
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        match r.u8()? {
            0 => Ok(ImageFill::Stretch),
            1 => Ok(ImageFill::Tile {
                scale: Wire::take(r)?,
            }),
            2 => Ok(ImageFill::Slice(Wire::take(r)?)),
            tag => Err(WireError::Tag {
                of: "ImageFill",
                tag,
            }),
        }
    }
}

wire_struct!(Linked { url, variants });
wire_struct!(Element {
    id,
    semantics,
    layout,
    rect,
    scroll_to,
    scrolls,
    primary_scroll,
    arrival_margin,
    picture,
    fixed_in_place_of,
    caret,
});

impl Wire for ElementId {
    fn put(&self, w: &mut Writer) {
        self.0.put(w);
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        u64::take(r).map(ElementId)
    }
}

const PICTURE_FITS: [&str; 5] = ["fill", "contain", "cover", "none", "scale-down"];

impl Wire for Picture {
    fn put(&self, w: &mut Writer) {
        self.source.put(w);
        self.width.put(w);
        self.height.put(w);
        w.str(self.fit);
        self.priority.put(w);
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        let source = Linked::take(r)?;
        let width = u32::take(r)?;
        let height = u32::take(r)?;
        let fit = r.str()?;
        let fit = PICTURE_FITS
            .into_iter()
            .find(|known| *known == fit)
            .ok_or(WireError::Invalid("an object-fit no document knows"))?;
        Ok(Picture {
            source,
            width,
            height,
            fit,
            priority: bool::take(r)?,
        })
    }
}
