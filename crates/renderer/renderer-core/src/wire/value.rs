//! How each value a draw command carries is spelled, field by field, with every enum's tags written out so the wire never follows declaration order. Geometry and semantics are spelled beside their own types, in `geometry-core` and `semantics-core`.

use semantics_core::wire::{LINK_SHAPE, put_link, take_link};
use wire_core::{
    Reader, Shaped, Wire, WireError, Writer, fold, fold_fields, fold_shape, shape_named, wire_enum,
    wire_struct,
};

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
    Stack(families: Self) = 7,
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

impl Shaped for Span {
    const SHAPE: u64 = fold_shape(
        fold(
            fold_fields!(shape_named("Span"), Span { range, over }),
            b"link",
        ),
        LINK_SHAPE,
    );
}

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

wire_enum!(GradientKind {
    Linear { start, end } = 0,
    Radial { transform } = 1,
});

impl Shaped for GradientStops {
    const SHAPE: u64 = fold_shape(shape_named("GradientStops"), <Vec<(f32, Color)>>::SHAPE);
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
        if len > GradientStops::CAPACITY {
            return Err(WireError::Invalid("more than 8 gradient stops"));
        }
        let stops: Vec<(f32, Color)> = (0..len)
            .map(|_| Ok((f32::take(r)?, Color::take(r)?)))
            .collect::<Result<_, WireError>>()?;
        Ok(GradientStops::new(&stops))
    }
}

impl Shaped for Dash {
    const SHAPE: u64 = fold_shape(
        fold_shape(shape_named("Dash"), <Vec<f32>>::SHAPE),
        f32::SHAPE,
    );
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

impl Shaped for Clamp {
    const SHAPE: u64 = {
        let hash = fold(fold(shape_named("Clamp"), b"None"), &[0]);
        let hash = fold(fold(hash, b"Lines"), &[1]);
        fold_shape(fold_shape(hash, u16::SHAPE), bool::SHAPE)
    };
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

impl Shaped for FontTag {
    const SHAPE: u64 = fold(shape_named("FontTag"), &[4]);
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

impl Shaped for FontVariations {
    const SHAPE: u64 = fold_shape(shape_named("FontVariations"), <Vec<(FontTag, f32)>>::SHAPE);
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

impl Shaped for FontFeatures {
    const SHAPE: u64 = fold_shape(shape_named("FontFeatures"), <Vec<(FontTag, u32)>>::SHAPE);
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

wire_enum!(PathVerb {
    MoveTo(to) = 0,
    LineTo(to) = 1,
    QuadTo { ctrl, to } = 2,
    CubicTo { ctrl1, ctrl2, to } = 3,
    Close = 4,
});

impl Shaped for PathData {
    const SHAPE: u64 = fold_shape(shape_named("PathData"), <Vec<PathVerb>>::SHAPE);
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

wire_enum!(ImageFill {
    Stretch = 0,
    Tile { scale } = 1,
    Slice(slice) = 2,
});

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

impl Shaped for ElementId {
    const SHAPE: u64 = fold_shape(shape_named("ElementId"), u64::SHAPE);
}

impl Wire for ElementId {
    fn put(&self, w: &mut Writer) {
        self.0.put(w);
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        u64::take(r).map(ElementId)
    }
}

const PICTURE_FITS: [&str; 5] = ["fill", "contain", "cover", "none", "scale-down"];

impl Shaped for Picture {
    const SHAPE: u64 = {
        let hash = fold_fields!(
            shape_named("Picture"),
            Picture {
                source,
                width,
                height
            }
        );
        let hash = fold_shape(fold(hash, b"fit"), String::SHAPE);
        fold_fields!(hash, Picture { priority })
    };
}

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
