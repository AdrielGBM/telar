//! How each value a draw command carries is spelled, field by field, with every enum's tags written out so the wire never follows declaration order.

use std::ops::Range;
use std::sync::Arc;

use geometry_core::{Point, Rect};
use semantics_core::{
    ConsumedKeys, Destination, HasPopup, Location, Modal, NumericValue, Orientation, SetPosition,
    SortDirection, Uri,
};

use super::WireError;
use super::bytes::{Reader, Writer};
use crate::{
    BlendMode, Border, BorderRadius, Clamp, Color, Dash, Declared, DecorationLength,
    DecorationLine, DecorationMetric, Element, ElementId, FillRule, Focusable, FontFamily,
    FontFeatures, FontStyle, FontTag, FontVariations, Gradient, GradientKind, GradientStops,
    ImageFill, ImageSlice, Insets, LayerMask, LineCap, LineHeight, LineJoin, Linked, Paint,
    PathData, PathStyle, PathVerb, Picture, Raster, RectStyle, Role, Semantics, Shadow, Span,
    Stroke, TextAlign, TextCase, TextDecoration, TextLength, TextShadow, TextStyle, TextWrap,
};

pub(crate) trait Wire: Sized {
    fn put(&self, w: &mut Writer);
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError>;
}

macro_rules! wire_struct {
    ($ty:ty { $($field:ident),+ $(,)? }) => {
        impl Wire for $ty {
            fn put(&self, w: &mut Writer) {
                $(self.$field.put(w);)+
            }
            fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
                Ok(Self { $($field: Wire::take(r)?),+ })
            }
        }
    };
}

macro_rules! wire_enum {
    ($ty:ident { $($variant:ident $(($field:ident: $field_ty:ty))? = $tag:literal),+ $(,)? }) => {
        impl Wire for $ty {
            fn put(&self, w: &mut Writer) {
                match self {
                    $($ty::$variant $(($field))? => {
                        w.u8($tag);
                        $($field.put(w);)?
                    })+
                }
            }
            fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
                match r.u8()? {
                    $($tag => Ok($ty::$variant $((<$field_ty as Wire>::take(r)?))?),)+
                    tag => Err(WireError::Tag { of: stringify!($ty), tag }),
                }
            }
        }
    };
}

macro_rules! wire_scalar {
    ($($ty:ty => $put:ident / $take:ident),+ $(,)?) => {
        $(impl Wire for $ty {
            fn put(&self, w: &mut Writer) {
                w.$put(*self);
            }
            fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
                r.$take()
            }
        })+
    };
}

wire_scalar!(
    bool => bool / bool,
    u8 => u8 / u8,
    u16 => u16 / u16,
    u64 => u64 / u64,
    f32 => f32 / f32,
    f64 => f64 / f64,
);

impl Wire for u32 {
    fn put(&self, w: &mut Writer) {
        w.varint(u64::from(*self));
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        r.varint_u32()
    }
}

impl Wire for Arc<str> {
    fn put(&self, w: &mut Writer) {
        w.str(self);
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        r.str().map(Arc::from)
    }
}

impl Wire for Box<str> {
    fn put(&self, w: &mut Writer) {
        w.str(self);
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        r.str().map(Box::from)
    }
}

impl<T: Wire> Wire for Option<T> {
    fn put(&self, w: &mut Writer) {
        match self {
            None => w.u8(0),
            Some(value) => {
                w.u8(1);
                value.put(w);
            }
        }
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        match r.u8()? {
            0 => Ok(None),
            1 => T::take(r).map(Some),
            tag => Err(WireError::Tag { of: "Option", tag }),
        }
    }
}

impl<T: Wire> Wire for Arc<T> {
    fn put(&self, w: &mut Writer) {
        T::put(self, w);
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        T::take(r).map(Arc::new)
    }
}

impl<T: Wire> Wire for Arc<[T]> {
    fn put(&self, w: &mut Writer) {
        w.len(self.len());
        for item in self.iter() {
            item.put(w);
        }
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        let len = r.len()?;
        (0..len).map(|_| T::take(r)).collect()
    }
}

impl<T: Wire, const N: usize> Wire for [T; N] {
    fn put(&self, w: &mut Writer) {
        for item in self {
            item.put(w);
        }
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        let items: Vec<T> = (0..N).map(|_| T::take(r)).collect::<Result<_, _>>()?;
        items
            .try_into()
            .map_err(|_| WireError::Invalid("fixed array length"))
    }
}

impl<A: Wire, B: Wire> Wire for (A, B) {
    fn put(&self, w: &mut Writer) {
        self.0.put(w);
        self.1.put(w);
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        Ok((A::take(r)?, B::take(r)?))
    }
}

impl Wire for Range<u32> {
    fn put(&self, w: &mut Writer) {
        self.start.put(w);
        self.end.put(w);
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        Ok(u32::take(r)?..u32::take(r)?)
    }
}

wire_struct!(Point { x, y });
wire_struct!(Rect {
    x,
    y,
    width,
    height
});
wire_struct!(Color { r, g, b, a });
wire_struct!(BorderRadius {
    top_left,
    top_right,
    bottom_right,
    bottom_left
});
wire_struct!(Insets {
    top,
    right,
    bottom,
    left
});

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
    Times(factor: f32) = 1
});
wire_enum!(TextShadow {
    None = 0,
    Cast(shadow: Shadow) = 1
});
wire_enum!(TextCase {
    AsWritten = 0,
    Upper = 1,
    Lower = 2,
    Capitalize = 3,
});
wire_enum!(TextLength {
    Px(value: f32) = 0,
    Em(value: f32) = 1,
    SurfaceWidth(value: f32) = 2,
    SurfaceHeight(value: f32) = 3,
    SurfaceMin(value: f32) = 4,
    SurfaceMax(value: f32) = 5,
});
wire_enum!(DecorationLine {
    None = 0,
    Underline = 1
});
wire_enum!(DecorationMetric {
    FromFont = 0,
    Px(px: f32) = 1
});
wire_enum!(DecorationLength {
    FromFont = 0,
    Length(length: TextLength) = 1
});
wire_enum!(FontFamily {
    SansSerif = 0,
    Serif = 1,
    Monospace = 2,
    SystemUi = 3,
    Cursive = 4,
    Fantasy = 5,
    Named(name: Arc<str>) = 6,
    Stack(families: Arc<[FontFamily]>) = 7,
});
wire_enum!(Paint {
    Solid(color: Color) = 0,
    Gradient(gradient: Gradient) = 1
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
wire_struct!(Span { range, over, link });
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

wire_enum!(Role {
    Group = 0,
    Banner = 1,
    Navigation = 2,
    Main = 3,
    Complementary = 4,
    ContentInfo = 5,
    Article = 6,
    Section = 7,
    Form = 8,
    Search = 9,
    Heading(level: u8) = 10,
    List = 11,
    ListItem = 12,
    ScrollArea = 13,
    Drawing = 14,
    Dialog = 15,
    Button = 16,
    Link = 17,
    CheckBox = 18,
    Radio = 19,
    Switch = 20,
    TabList = 21,
    Tab = 22,
    TabPanel = 23,
    Menu = 24,
    MenuItem = 25,
    MenuItemCheckBox = 26,
    MenuItemRadio = 27,
    ListBox = 28,
    Option = 29,
    Slider = 30,
    SpinButton = 31,
    TextInput = 32,
    MultilineTextInput = 33,
    ComboBox = 34,
    Disclosure = 35,
    ProgressBar = 36,
    Tree = 37,
    Grid = 38,
    Row = 39,
    GridCell = 40,
    ColumnHeader = 41,
    TreeItem = 42,
    Toolbar = 43,
    Splitter = 44,
    Status = 45,
    Log = 46,
    Label = 47,
});
wire_enum!(Orientation {
    Horizontal = 0,
    Vertical = 1
});
wire_enum!(SortDirection {
    Ascending = 0,
    Descending = 1
});
wire_enum!(HasPopup {
    Menu = 0,
    ListBox = 1
});
wire_enum!(Destination {
    Route(location: Location) = 0,
    Anchor(name: Arc<str>) = 1,
    Locale(locale: Arc<str>) = 2,
    External(uri: Uri) = 3,
});

wire_struct!(NumericValue { now, min, max });
wire_struct!(SetPosition {
    level,
    position,
    size
});
wire_struct!(Modal { layer, opened_from });
wire_struct!(Focusable { tab_stop, consumes });
wire_struct!(Semantics {
    role,
    label,
    link,
    focused,
    toggled,
    disabled,
    click_through,
    focusable,
    lang,
    hidden,
    anchor,
    current,
    value,
    orientation,
    expanded,
    position,
    active_descendant,
    controls,
    has_popup,
    keyboard_shortcut,
    sort,
    multiselectable,
    modal,
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

impl Wire for ElementId {
    fn put(&self, w: &mut Writer) {
        self.0.put(w);
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        u64::take(r).map(ElementId)
    }
}

impl Wire for ConsumedKeys {
    fn put(&self, w: &mut Writer) {
        self.bits().put(w);
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        u16::take(r).map(ConsumedKeys::from_bits)
    }
}

// The host parses the address again rather than trusting the guest's parse, so its own scheme policy is the one that decides.
impl Wire for Uri {
    fn put(&self, w: &mut Writer) {
        w.str(self.as_str());
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        Uri::parse(r.str()?).map_err(|_| WireError::Invalid("a link the host does not accept"))
    }
}

impl Wire for Location {
    fn put(&self, w: &mut Writer) {
        w.len(self.segments().len());
        for segment in self.segments() {
            w.str(segment);
        }
        self.locale().map(Box::<str>::from).put(w);
        self.fragment().map(Box::<str>::from).put(w);
        w.len(self.params().len());
        for (key, value) in self.params() {
            w.str(key);
            w.str(value);
        }
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        let len = r.len()?;
        let segments: Vec<&str> = (0..len).map(|_| r.str()).collect::<Result<_, _>>()?;
        let mut location = Location::from_segments(segments);
        if let Some(locale) = <Option<Box<str>> as Wire>::take(r)? {
            location = location.with_locale(locale);
        }
        if let Some(fragment) = <Option<Box<str>> as Wire>::take(r)? {
            location = location.with_fragment(fragment);
        }
        let len = r.len()?;
        for _ in 0..len {
            location = location.with_param(r.str()?, r.str()?);
        }
        Ok(location)
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
