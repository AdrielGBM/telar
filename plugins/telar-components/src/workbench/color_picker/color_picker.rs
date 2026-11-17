//! [`color_picker`]: a colour chosen on a saturation and brightness area, hue and opacity sliders, a hex field and a row of swatches.

use std::rc::Rc;

use telar::{
    Accessible, AlignItems, Border, BorderRadius, Canvas, Clip, ClippedItem, Color, Container,
    Cursor, Gradient, Handler, Key, LayoutError, LayoutItem, LayoutStyle, ModifiersState, NamedKey,
    NumericValue, Orientation, Paint, Point, Props, Reactive, Rect, RectStyle, RenderNode,
    RovingAxis, RovingGroup, RwSignal, ShapeStyle, StyledContainer, SurfaceAmendment,
    amend_surface, box_item, box_transform, children, effect,
    focus::{self, FocusId, Role, Step},
    signal, step_factor,
};

use crate::inputs::swatches::swatches::{SwatchesProps, swatches};
use crate::inputs::text_field::text_field::{TextFieldProps, text_field};
use crate::shared;
use crate::strings;
use telar::ControlSize;

/// A colour picker's own tokens, which an application's theme supplies through a `#[theme(extension)]` field.
///
/// Every field left `None` follows the role named beside it, read when the picker paints.
#[derive(Clone, Debug, Default, PartialEq, telar::ComponentTokens)]
pub struct ColorPickerTokens {
    /// The width a picker takes where what holds it leaves the room, at the regular size and scaled with the picker's own: narrower where it does not, and as its call places it (`width:100%`). `None` is 16 ems of the document's text, a slider's length.
    pub width: Option<f32>,
    /// Corner radius of the saturation and brightness area and of the chip showing the colour. `None` is the theme's `radius_control`.
    #[token(or = shared::radius_control)]
    pub radius: Option<f32>,
    /// Width of the ring round each thumb. `None` is the theme's `border_width_strong`.
    #[token(or = shared::border_width_strong)]
    pub ring: Option<f32>,
}

const WIDTH_EMS: f32 = 16.0;

impl ColorPickerTokens {
    /// The width a picker of `size` takes where what holds it leaves the room.
    pub fn width(&self, size: ControlSize) -> f32 {
        shared::width_or_measure(size, self.width, WIDTH_EMS)
    }
}

const AREA_RATIO: f32 = 0.62;
/// One arrow-key step on any channel, as a share of its range.
const KEY_STEP: f32 = 0.01;

fn gap(size: ControlSize) -> f32 {
    shared::gap_md(size)
}
fn thumb_size(size: ControlSize) -> f32 {
    shared::icon_size(size)
}
/// A channel's track sits inside its thumb by the smallest step on each side.
fn track_height(size: ControlSize) -> f32 {
    thumb_size(size) - shared::gap_xs(size) * 2.0
}
/// The chip spans both channel tracks and the gap between them.
fn chip_size(size: ControlSize) -> f32 {
    track_height(size) * 2.0 + gap(size)
}
fn checker_cell(size: ControlSize) -> f32 {
    shared::gap_sm(size)
}

fn ring(color: Color) -> Border {
    Border::uniform(color, ColorPickerTokens::current().ring())
}

fn rounded() -> BorderRadius {
    BorderRadius::all(ColorPickerTokens::current().radius())
}

/// A colour picked on a saturation and brightness area, a hue slider and, unless turned off, an opacity slider; typed as hex; or chosen from a row of swatches.
#[derive(Props)]
pub struct ColorPickerProps {
    /// The bound colour. `None` (the default) is uncontrolled, starting at the theme accent.
    #[props(some, into, default)]
    pub value: Option<RwSignal<Color>>,
    /// Offers an opacity slider and reads and writes the hex with its alpha. On by default; off, the colour's alpha is left as it was.
    #[props(default = true)]
    pub alpha: bool,
    /// Colours offered as swatches under the controls, in order: a theme's own tokens, say. None by default.
    #[props(default)]
    pub swatches: Vec<Color>,
    /// What each swatch is called, for a reader; one per colour.
    #[props(default)]
    pub swatch_names: Vec<String>,
    /// How large the picker is drawn: its thumbs, tracks, chip, hex field, swatches, caption and gaps, and the width it takes where what holds it leaves the room, as [`ControlSize`] works them out.
    #[props(default)]
    pub size: ControlSize,
    /// A small caption stacked above the picker; omitted when empty.
    #[props(into, default)]
    pub label: Reactive<String>,
    /// Fires with the colour every drag, key, typed hex or swatch picks.
    #[props(some, default)]
    pub on_change: Option<Handler<fn(Color)>>,
    /// Amends the paint of the saturation and brightness area — the picker's **principal surface**. See [`telar::SurfaceStyle`] for when a theme token or [`ColorPickerTokens`] is the right instrument instead.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
}

/// A colour in hue (degrees), saturation and value: the picker's own model, which keeps the hue a grey or a black has no way to say.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Hsv {
    pub h: f32,
    pub s: f32,
    pub v: f32,
}

/// The colour's hue in degrees, or `None` for a grey, which has none; then its saturation and value.
pub(crate) fn to_hsv(color: Color) -> (Option<f32>, f32, f32) {
    let (r, g, b) = (color.r, color.g, color.b);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let s = if max > 0.0 { delta / max } else { 0.0 };
    let h = if delta <= f32::EPSILON {
        None
    } else if max == r {
        Some(60.0 * ((g - b) / delta).rem_euclid(6.0))
    } else if max == g {
        Some(60.0 * ((b - r) / delta + 2.0))
    } else {
        Some(60.0 * ((r - g) / delta + 4.0))
    };
    (h, s, max)
}

pub(crate) fn from_hsv(hsv: Hsv, alpha: f32) -> Color {
    let Hsv { h, s, v } = hsv;
    let c = v * s;
    let sector = (h.rem_euclid(360.0)) / 60.0;
    let x = c * (1.0 - (sector.rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = match sector as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    Color::rgba(r + m, g + m, b + m, alpha)
}

/// `#rrggbb`, or `#rrggbbaa` when the alpha is shown and the colour is not opaque.
pub(crate) fn hex_of(color: Color, with_alpha: bool) -> String {
    let [r, g, b, a] = color.to_rgba8();
    if with_alpha && a < 255 {
        format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
    } else {
        format!("#{r:02x}{g:02x}{b:02x}")
    }
}

fn same_rgb(a: Color, b: Color) -> bool {
    a.to_rgba8()[..3] == b.to_rgba8()[..3]
}

#[derive(Clone)]
struct Picker {
    size: ControlSize,
    value: RwSignal<Color>,
    hsv: RwSignal<Hsv>,
    on_change: Option<Handler<fn(Color)>>,
}

impl Picker {
    /// The colour's HSV, keeping the hue (and, for black, the saturation) it was shown with where the colour itself no longer says one.
    fn hsv_keeping(color: Color, shown: Hsv) -> Hsv {
        let (hue, s, v) = to_hsv(color);
        Hsv {
            h: hue.unwrap_or(shown.h),
            s: if v <= 0.0 { shown.s } else { s },
            v,
        }
    }

    fn commit(&self, color: Color) {
        self.value.set_if_changed(color);
        if let Some(on_change) = &self.on_change {
            on_change.run(color);
        }
    }

    fn set_hsv(&self, hsv: Hsv) {
        self.hsv.set(hsv);
        self.commit(from_hsv(hsv, self.value.peek().a));
    }

    fn set_alpha(&self, alpha: f32) {
        self.commit(self.value.peek().with_alpha(alpha.clamp(0.0, 1.0)));
    }

    fn set_color(&self, color: Color) {
        self.hsv.set(Self::hsv_keeping(color, self.hsv.peek()));
        self.commit(color);
    }

    /// Follows a colour written from outside, leaving the shown hue alone when it is the same colour.
    fn follow(&self, color: Color) {
        let shown = self.hsv.peek();
        if !same_rgb(from_hsv(shown, color.a), color) {
            self.hsv.set(Self::hsv_keeping(color, shown));
        }
    }
}

/// A colour picked on a saturation and brightness area, hue and opacity sliders, a hex field and a row of swatches.
pub fn color_picker(props: ColorPickerProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let ColorPickerProps {
        value,
        alpha,
        swatches: palette,
        swatch_names,
        size,
        label,
        on_change,
        style,
    } = props;
    let value = value.unwrap_or_else(|| signal(shared::accent()));
    let start = value.peek();
    let picker = Picker {
        size,
        value,
        hsv: signal(Picker::hsv_keeping(
            start,
            Hsv {
                h: 0.0,
                s: 0.0,
                v: 0.0,
            },
        )),
        on_change,
    };
    {
        let picker = picker.clone();
        effect(move || picker.follow(value.get()));
    }

    let mut rows: Vec<Box<dyn LayoutItem>> = vec![area(&picker, style)?];
    let mut sliders: Vec<Box<dyn LayoutItem>> = vec![hue_slider(&picker)?];
    if alpha {
        sliders.push(alpha_slider(&picker)?);
    }
    let slider_column = Container::new(
        LayoutStyle::column()
            .flex_grow(1.0)
            .min_width(0.0)
            .justify_content(telar::JustifyContent::CENTER),
        sliders,
    )?;
    rows.push(box_item(Container::new(
        LayoutStyle::row()
            .align_items(AlignItems::CENTER)
            .gap(gap(size)),
        vec![chip(size, value)?, box_item(slider_column)],
    )?));
    rows.push(hex_field(&picker, alpha)?);
    if !palette.is_empty() {
        rows.push(swatch_row(&picker, palette, swatch_names)?);
    }

    let width = move || ColorPickerTokens::current().width(size);
    let captioned = shared::is_captioned(&label);
    let column = move || {
        let column = LayoutStyle::column().gap(gap(size));
        match captioned {
            true => column,
            false => shared::preferred_width(column, width()),
        }
    };
    let picker_box = Container::new(column(), rows)?.styled_by(column);
    shared::captioned(
        size,
        box_item(picker_box),
        label,
        Some(Reactive::of(width)),
        false,
    )
}

/// One of the area's two axes, each a slider of its own to a reader: saturation runs across it, brightness up it.
#[derive(Clone, Copy, PartialEq)]
enum Axis {
    Saturation,
    Brightness,
}

impl Axis {
    fn of(self, hsv: Hsv) -> f32 {
        match self {
            Self::Saturation => hsv.s,
            Self::Brightness => hsv.v,
        }
    }

    fn set(self, hsv: Hsv, to: f32) -> Hsv {
        let to = to.clamp(0.0, 1.0);
        match self {
            Self::Saturation => Hsv { s: to, ..hsv },
            Self::Brightness => Hsv { v: to, ..hsv },
        }
    }

    /// Its place in the area's roving group, and among the area's sliders.
    fn index(self) -> usize {
        match self {
            Self::Saturation => 0,
            Self::Brightness => 1,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Saturation => strings::SATURATION,
            Self::Brightness => strings::BRIGHTNESS,
        }
    }

    fn orientation(self) -> Orientation {
        match self {
            Self::Saturation => Orientation::Horizontal,
            Self::Brightness => Orientation::Vertical,
        }
    }

    /// The axis an arrow key moves along, and which way.
    fn of_arrow(key: &NamedKey) -> Option<(Self, f32)> {
        match key {
            NamedKey::ArrowRight => Some((Self::Saturation, 1.0)),
            NamedKey::ArrowLeft => Some((Self::Saturation, -1.0)),
            NamedKey::ArrowUp => Some((Self::Brightness, 1.0)),
            NamedKey::ArrowDown => Some((Self::Brightness, -1.0)),
            _ => None,
        }
    }
}

/// Makes `layer` the slider for `axis`: one stop of the area's `group`, valued and stepped along its axis alone.
fn axis_control(
    layer: StyledContainer,
    axis: Axis,
    picker: &Picker,
    group: &RovingGroup,
) -> StyledContainer {
    let (announced, stepped) = (picker.clone(), picker.clone());
    layer
        .control(Role::Slider)
        .roving(group, axis.index())
        .a11y_label(move || strings::text(axis.name()))
        .oriented(axis.orientation())
        .valued(move || {
            NumericValue::new(
                0.0..=100.0,
                (f64::from(axis.of(announced.hsv.get())) * 100.0).round(),
            )
            .ok()
        })
        .stepped(move |direction| {
            let hsv = stepped.hsv.peek();
            let before = axis.of(hsv);
            let by = match direction {
                Step::Increment => KEY_STEP,
                Step::Decrement => -KEY_STEP,
            };
            let next = axis.set(hsv, before + by);
            stepped.set_hsv(next);
            axis.of(next) != before
        })
}

/// What a key does on the slider for `own`: the arrows move the axis they point along and hand focus to its slider, so a reader hears the value that moved; Home and End take `own` to its ends.
fn area_key(picker: &Picker, own: Axis, sliders: [Option<FocusId>; 2], key: &Key) -> bool {
    let Key::Named(named) = key else {
        return false;
    };
    let hsv = picker.hsv.peek();
    let (axis, next) = match named {
        NamedKey::Home => (own, own.set(hsv, 0.0)),
        NamedKey::End => (own, own.set(hsv, 1.0)),
        arrow => match Axis::of_arrow(arrow) {
            Some((axis, way)) => {
                let step = KEY_STEP * step_factor(telar::modifiers()) * way;
                (axis, axis.set(hsv, axis.of(hsv) + step))
            }
            None => return false,
        },
    };
    picker.set_hsv(next);
    if axis != own
        && let Some(id) = sliders[axis.index()]
    {
        focus::request(id);
    }
    true
}

/// The saturation (across) and brightness (up) of the current hue, dragged as one or stepped as two sliders, one per axis, that are one Tab stop between them.
fn area(
    picker: &Picker,
    style: Option<SurfaceAmendment>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let hsv = picker.hsv;
    let group = RovingGroup::new(RovingAxis::Both, || Some(0), |_| {});
    let whiten = StyledContainer::new(
        LayoutStyle::new().absolute_fill(),
        |r: Rect| {
            RectStyle::default()
                .with_fill(Paint::Gradient(Gradient::linear(
                    Point::new(r.x, r.y),
                    Point::new(r.x + r.width, r.y),
                    &[(0.0, Color::WHITE), (1.0, Color::WHITE.with_alpha(0.0))],
                )))
                .with_radius(rounded())
        },
        vec![],
    )?;
    // The whitening layer lies over the whole area and exists before it, so its laid rect is what the thumb is placed in and a drag is measured against.
    let area_rect =
        telar::track_layout(whiten.layout_node()).expect("the layer's node is registered");
    let whiten = axis_control(whiten, Axis::Saturation, picker, &group);
    let darken = StyledContainer::new(
        LayoutStyle::new().absolute_fill(),
        |r: Rect| {
            RectStyle::default()
                .with_fill(Paint::Gradient(Gradient::linear(
                    Point::new(r.x, r.y),
                    Point::new(r.x, r.y + r.height),
                    &[(0.0, Color::BLACK.with_alpha(0.0)), (1.0, Color::BLACK)],
                )))
                .with_radius(rounded())
        },
        vec![],
    )?;
    let darken = axis_control(darken, Axis::Brightness, picker, &group);
    let sliders = [whiten.focus_id(), darken.focus_id()];
    let whiten = {
        let keys = picker.clone();
        whiten.on_focused_key(move |key: &Key| area_key(&keys, Axis::Saturation, sliders, key))
    };
    let darken = {
        let keys = picker.clone();
        darken.on_focused_key(move |key: &Key| area_key(&keys, Axis::Brightness, sliders, key))
    };
    let size = thumb_size(picker.size);
    let thumb = StyledContainer::new(
        LayoutStyle::new().absolute().width(size).height(size),
        move |r| {
            let shown = from_hsv(hsv.get(), 1.0);
            RectStyle::default()
                .with_fill(shown)
                .with_border(ring(shared::thumb_on(shown)))
                .with_radius(shared::pill(r))
        },
        vec![],
    )?
    .with_transform(move |r| {
        let Hsv { s, v, .. } = hsv.get();
        let area = area_rect.get();
        box_transform(
            r,
            0.0,
            1.0,
            1.0,
            s * area.width - size / 2.0,
            (1.0 - v) * area.height - size / 2.0,
        )
    })
    .a11y_hidden();

    let drag = picker.clone();
    let area = StyledContainer::new(
        LayoutStyle::new()
            .width(telar::SizeDimension::Percent(1.0))
            .aspect_ratio(1.0 / AREA_RATIO),
        move |_r| {
            let hue = from_hsv(
                Hsv {
                    h: hsv.get().h,
                    s: 1.0,
                    v: 1.0,
                },
                1.0,
            );
            amend_surface(
                RectStyle::default().with_fill(hue).with_radius(rounded()),
                &style,
            )
        },
        children![whiten, darken, thumb],
    )?
    .colour_content()
    .a11y_label(|| strings::text(strings::SATURATION_BRIGHTNESS))
    .cursor(Cursor::Crosshair)
    .on_drag(move |x, y| {
        let h = drag.hsv.peek().h;
        let area = area_rect.peek();
        drag.set_hsv(Hsv {
            h,
            s: (x / area.width).clamp(0.0, 1.0),
            v: (1.0 - y / area.height).clamp(0.0, 1.0),
        });
    });
    Ok(box_item(area))
}

/// One channel along a track: where it stands as `0..=1`, how to set it, what its track is painted with, and what a reader is told.
struct Channel {
    read: Rc<dyn Fn() -> f32>,
    write: Handler<fn(f32)>,
    paint: Rc<dyn Fn(Rect) -> Paint>,
    /// The colour the track shows under the thumb, which the thumb's ring has to stand out from.
    under: Rc<dyn Fn() -> Color>,
    /// The picker's size, which the track, its thumb and its checker are drawn at.
    size: ControlSize,
    checkered: bool,
    name: &'static str,
    /// The announced range: degrees for the hue, a percentage for the opacity.
    range: f64,
}

fn hue_slider(picker: &Picker) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (read, write, under) = (picker.clone(), picker.clone(), picker.clone());
    channel(Channel {
        read: Rc::new(move || read.hsv.get().h / 360.0),
        write: Handler::from(move |t: f32| {
            let hsv = write.hsv.peek();
            write.set_hsv(Hsv {
                h: (t * 360.0).clamp(0.0, 360.0),
                ..hsv
            });
        }),
        paint: Rc::new(|r: Rect| {
            let stops: Vec<(f32, Color)> = (0..=6)
                .map(|i| {
                    let t = i as f32 / 6.0;
                    let hue = Hsv {
                        h: t * 360.0,
                        s: 1.0,
                        v: 1.0,
                    };
                    (t, from_hsv(hue, 1.0))
                })
                .collect();
            Paint::Gradient(Gradient::linear(
                Point::new(r.x, r.y),
                Point::new(r.x + r.width, r.y),
                &stops,
            ))
        }),
        under: Rc::new(move || {
            let h = under.hsv.get().h;
            from_hsv(Hsv { h, s: 1.0, v: 1.0 }, 1.0)
        }),
        size: picker.size,
        checkered: false,
        name: strings::HUE,
        range: 360.0,
    })
}

fn alpha_slider(picker: &Picker) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (read, write, paint, under) = (
        picker.clone(),
        picker.clone(),
        picker.clone(),
        picker.clone(),
    );
    channel(Channel {
        read: Rc::new(move || read.value.get().a),
        write: Handler::from(move |t: f32| write.set_alpha(t)),
        paint: Rc::new(move |r: Rect| {
            let opaque = paint.value.get().with_alpha(1.0);
            Paint::Gradient(Gradient::linear(
                Point::new(r.x, r.y),
                Point::new(r.x + r.width, r.y),
                &[(0.0, opaque.with_alpha(0.0)), (1.0, opaque)],
            ))
        }),
        under: Rc::new(move || under.value.get().over(shared::surface())),
        size: picker.size,
        checkered: true,
        name: strings::OPACITY,
        range: 100.0,
    })
}

fn channel(channel: Channel) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let Channel {
        read,
        write,
        paint,
        under,
        size: control_size,
        checkered,
        name,
        range,
    } = channel;
    let track_height = track_height(control_size);
    let mut layers: Vec<Box<dyn LayoutItem>> = Vec::new();
    if checkered {
        layers.push(checker(control_size, move || {
            BorderRadius::all(shared::pill_radius(track_height))
        })?);
    }
    let face = StyledContainer::new(
        LayoutStyle::new().absolute_fill(),
        move |r| {
            RectStyle::default()
                .with_fill(paint(r))
                .with_border(Border::uniform(shared::border(), shared::border_width()))
                .with_radius(shared::pill(r))
        },
        vec![],
    )?;
    // The face lies over the whole track and exists before it, so its laid rect is the length the thumb travels and a drag is measured against.
    let track_rect =
        telar::track_layout(face.layout_node()).expect("the face's node is registered");
    layers.push(box_item(face));
    let size = thumb_size(control_size);
    let at = read.clone();
    layers.push(box_item(
        StyledContainer::new(
            LayoutStyle::new().absolute().width(size).height(size),
            move |r| {
                RectStyle::default()
                    .with_border(ring(shared::thumb_on(under())))
                    .with_radius(shared::pill(r))
            },
            vec![],
        )?
        .with_transform(move |r| {
            let t = at().clamp(0.0, 1.0);
            box_transform(
                r,
                0.0,
                1.0,
                1.0,
                t * (track_rect.get().width - size),
                (track_height - size) / 2.0,
            )
        })
        .a11y_hidden(),
    ));

    let (announced, stepped, keyed) = (read.clone(), read.clone(), read);
    let (drag, set) = (write.clone(), write.clone());
    let track = Container::new(LayoutStyle::new().height(track_height), layers)?.colour_content();
    let track = StyledContainer::new(
        LayoutStyle::column()
            .height(shared::MIN_TARGET)
            .justify_content(telar::JustifyContent::CENTER),
        |_r| RectStyle::default(),
        children![track],
    )?
    .control(Role::Slider)
    .a11y_label(move || strings::text(name))
    .valued(move || NumericValue::new(0.0..=range, (f64::from(announced()) * range).round()).ok())
    .stepped(move |direction| {
        let before = stepped();
        let next = match direction {
            telar::focus::Step::Increment => before + KEY_STEP,
            telar::focus::Step::Decrement => before - KEY_STEP,
        }
        .clamp(0.0, 1.0);
        set.run(next);
        next != before
    })
    .on_focused_key(move |key: &Key| {
        let step = KEY_STEP * step_factor(telar::modifiers());
        let next = match key {
            Key::Named(NamedKey::ArrowRight | NamedKey::ArrowUp) => keyed() + step,
            Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowDown) => keyed() - step,
            Key::Named(NamedKey::Home) => 0.0,
            Key::Named(NamedKey::End) => 1.0,
            _ => return false,
        };
        write.run(next.clamp(0.0, 1.0));
        true
    })
    .on_drag(move |x, _| drag.run((x / track_rect.peek().width).clamp(0.0, 1.0)));
    Ok(box_item(track))
}

/// The squares a translucent colour is shown over, so how much shows through can be seen, filling its parent and cut to the `corner` of what is drawn over it.
fn checker(
    size: ControlSize,
    corner: impl Fn() -> BorderRadius + 'static,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let canvas = Canvas::new(LayoutStyle::new().absolute_fill(), move |rect| {
        let light = shared::surface();
        let dark = shared::border().over(light);
        let cell = checker_cell(size);
        let columns = (rect.width / cell).ceil() as usize;
        let rows = (rect.height / cell).ceil() as usize;
        let mut cells = vec![RenderNode::rect(
            rect,
            RectStyle::default().with_fill(light),
        )];
        for row in 0..rows {
            for column in (row % 2..columns).step_by(2) {
                let x = column as f32 * cell;
                let y = row as f32 * cell;
                cells.push(RenderNode::rect(
                    Rect::new(x, y, cell.min(rect.width - x), cell.min(rect.height - y)),
                    RectStyle::default().with_fill(dark),
                ));
            }
        }
        RenderNode::group(cells)
    })?
    .a11y_hidden();
    let cut = ClippedItem::following(box_item(canvas), move || {
        Clip::both().rounded(corner()).paint_only()
    });
    Ok(box_item(cut))
}

/// The colour as it stands, alpha and all, over the checker.
fn chip(
    control_size: ControlSize,
    value: RwSignal<Color>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let size = chip_size(control_size);
    let face = StyledContainer::new(
        LayoutStyle::new().absolute_fill(),
        move |_r| {
            RectStyle::default()
                .with_fill(value.get())
                .with_border(Border::uniform(shared::border(), shared::border_width()))
                .with_radius(rounded())
        },
        vec![],
    )?;
    let chip = Container::new(
        LayoutStyle::new().width(size).height(size).flex_shrink(0.0),
        vec![checker(control_size, rounded)?, box_item(face)],
    )?
    .colour_content()
    .a11y_hidden();
    Ok(box_item(chip))
}

/// The colour as hex: rewritten whenever the colour moves, and applied on Enter or Tab when it parses. Escape, or text that does not parse, puts back the colour's own.
fn hex_field(picker: &Picker, alpha: bool) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let value = picker.value;
    let text = signal(hex_of(value.peek(), alpha));
    effect(move || {
        let shown = hex_of(value.get(), alpha);
        text.set_if_changed(shown);
    });
    let apply = {
        let picker = picker.clone();
        Handler::from(move || {
            let typed = text.peek();
            match Color::from_hex(typed.trim()) {
                Some(parsed) => {
                    let parsed = if alpha {
                        parsed
                    } else {
                        parsed.with_alpha(picker.value.peek().a)
                    };
                    picker.set_color(parsed);
                    text.set(hex_of(parsed, alpha));
                }
                None => text.set(hex_of(picker.value.peek(), alpha)),
            }
        })
    };
    let on_key = {
        let apply = apply.clone();
        Rc::new(move |key: &Key, _: ModifiersState| {
            match key {
                Key::Named(NamedKey::Tab) => apply.run(),
                Key::Named(NamedKey::Escape) => text.set(hex_of(value.peek(), alpha)),
                _ => {}
            }
            false
        })
    };
    let field = text_field(
        TextFieldProps::props()
            .value(text)
            .placeholder(Reactive::of(|| strings::text(strings::HEX)))
            .size(picker.size)
            .maybe_on_submit(Some(apply))
            .on_key(on_key)
            .build(),
    )?;
    telar::place_item(field.layout_node(), |style| {
        style.width(telar::SizeDimension::Percent(1.0))
    });
    Ok(field)
}

fn swatch_row(
    picker: &Picker,
    palette: Vec<Color>,
    names: Vec<String>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let value = picker.value;
    let selected = signal(None::<u32>);
    {
        let palette = palette.clone();
        effect(move || {
            let current = value.get();
            let at = palette
                .iter()
                .position(|swatch| swatch.to_rgba8() == current.to_rgba8())
                .map(|i| i as u32);
            selected.set_if_changed(at);
        });
    }
    let choose = {
        let (picker, palette) = (picker.clone(), palette.clone());
        move |index: u32| {
            if let Some(&color) = palette.get(index as usize) {
                picker.set_color(color);
            }
        }
    };
    swatches(
        SwatchesProps::props()
            .colors(palette)
            .names(names)
            .label(Reactive::of(|| strings::text(strings::SWATCHES)))
            .selected(selected)
            .on_select(choose)
            .size(picker.size)
            .build(),
    )
}

#[cfg(test)]
#[path = "color_picker.test.rs"]
mod tests;
