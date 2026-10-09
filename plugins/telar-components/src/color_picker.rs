//! [`color_picker`]: a colour chosen on a saturation and brightness area, hue and opacity sliders, a hex field and a row of swatches.

use std::rc::Rc;

use telar::{
    Accessible, AlignItems, Border, BorderRadius, Canvas, Children, Color, Container, Cursor,
    Gradient, Key, LayoutError, LayoutItem, LayoutStyle, ModifiersState, NamedKey, NumericValue,
    Paint, Point, Props, Reactive, Rect, RectStyle, RenderNode, RwSignal, ShapeStyle,
    StyledContainer, box_item, box_transform, effect, focus::Role, signal, step_factor,
};

use crate::shared;
use crate::strings;
use crate::swatches::{SwatchesProps, swatches};
use crate::text_field::{TextFieldProps, text_field};

const DEFAULT_WIDTH: f32 = 220.0;
const AREA_RATIO: f32 = 0.62;
const TRACK_HEIGHT: f32 = 12.0;
const THUMB_RING: f32 = 2.0;
const CHECKER_CELL: f32 = 4.0;
const SWATCH_SIZE: f32 = 18.0;
/// One arrow-key step on any channel, as a share of its range.
const KEY_STEP: f32 = 0.01;

fn gap() -> f32 {
    shared::spacing()
}
fn chip_size() -> f32 {
    TRACK_HEIGHT * 2.0 + gap()
}
fn thumb_size() -> f32 {
    TRACK_HEIGHT + 4.0
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
    /// Width in logical px. `0.0` (the default) means 220.
    #[props(default)]
    pub width: f32,
    /// A small caption stacked above the picker; omitted when empty.
    #[props(into, default)]
    pub label: Reactive<String>,
    /// Fires with the colour every drag, key, typed hex or swatch picks.
    #[props(some, default)]
    pub on_change: Option<Rc<dyn Fn(Color)>>,
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
    value: RwSignal<Color>,
    hsv: RwSignal<Hsv>,
    on_change: Option<Rc<dyn Fn(Color)>>,
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
            on_change(color);
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
pub fn color_picker(
    props: ColorPickerProps,
    _children: Children,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let ColorPickerProps {
        value,
        alpha,
        swatches: palette,
        swatch_names,
        width,
        label,
        on_change,
    } = props;
    let value = value.unwrap_or_else(|| signal(shared::accent()));
    let width = if width > 0.0 { width } else { DEFAULT_WIDTH };
    let start = value.peek();
    let picker = Picker {
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

    let mut rows: Vec<Box<dyn LayoutItem>> = vec![area(&picker, width)?];
    let slider_width = width - chip_size() - gap();
    let mut sliders: Vec<Box<dyn LayoutItem>> = vec![hue_slider(&picker, slider_width)?];
    if alpha {
        sliders.push(alpha_slider(&picker, slider_width)?);
    }
    let slider_column = Container::new(
        LayoutStyle::new()
            .flex_column()
            .gap(gap())
            .justify_content(telar::JustifyContent::CENTER),
        sliders,
    )?;
    rows.push(box_item(Container::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .gap(gap()),
        vec![chip(value)?, box_item(slider_column)],
    )?));
    rows.push(hex_field(&picker, width, alpha)?);
    if !palette.is_empty() {
        rows.push(swatch_row(&picker, palette, swatch_names)?);
    }

    let column = move || LayoutStyle::new().flex_column().width(width).gap(gap());
    let picker_box = Container::new(column(), rows)?.styled_by(column);
    shared::captioned(box_item(picker_box), label, width)
}

fn ring() -> Border {
    Border::uniform(Color::WHITE, THUMB_RING)
}

/// The saturation (across) and brightness (up) of the current hue, dragged or stepped with the arrows.
fn area(picker: &Picker, width: f32) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let height = (width * AREA_RATIO).round();
    let hsv = picker.hsv;
    let whiten = StyledContainer::new(
        LayoutStyle::new().absolute_fill(),
        |r: Rect| {
            RectStyle::default()
                .with_fill(Paint::Gradient(Gradient::linear(
                    Point::new(r.x, r.y),
                    Point::new(r.x + r.width, r.y),
                    &[(0.0, Color::WHITE), (1.0, Color::WHITE.with_alpha(0.0))],
                )))
                .with_radius(BorderRadius::all(shared::radius()))
        },
        vec![],
    )?;
    let darken = StyledContainer::new(
        LayoutStyle::new().absolute_fill(),
        |r: Rect| {
            RectStyle::default()
                .with_fill(Paint::Gradient(Gradient::linear(
                    Point::new(r.x, r.y),
                    Point::new(r.x, r.y + r.height),
                    &[(0.0, Color::BLACK.with_alpha(0.0)), (1.0, Color::BLACK)],
                )))
                .with_radius(BorderRadius::all(shared::radius()))
        },
        vec![],
    )?;
    let size = thumb_size();
    let thumb = StyledContainer::new(
        LayoutStyle::new().absolute().width(size).height(size),
        move |_r| {
            RectStyle::default()
                .with_fill(from_hsv(hsv.get(), 1.0))
                .with_border(ring())
                .with_radius(BorderRadius::all(size / 2.0))
        },
        vec![],
    )?
    .with_transform(move |r| {
        let Hsv { s, v, .. } = hsv.get();
        box_transform(
            r,
            0.0,
            1.0,
            1.0,
            s * width - size / 2.0,
            (1.0 - v) * height - size / 2.0,
        )
    })
    .a11y_hidden();

    let keys = picker.clone();
    let drag = picker.clone();
    let area = StyledContainer::new(
        LayoutStyle::new().width(width).height(height),
        move |_r| {
            RectStyle::default()
                .with_fill(from_hsv(
                    Hsv {
                        h: hsv.get().h,
                        s: 1.0,
                        v: 1.0,
                    },
                    1.0,
                ))
                .with_radius(BorderRadius::all(shared::radius()))
        },
        vec![box_item(whiten), box_item(darken), box_item(thumb)],
    )?
    .control(Role::Slider)
    .a11y_label(|| strings::text(strings::SATURATION_BRIGHTNESS))
    .valued(move || NumericValue {
        now: (hsv.get().s * 100.0).round() as f64,
        min: 0.0,
        max: 100.0,
    })
    .cursor(Cursor::Crosshair)
    .on_focused_key(move |key: &Key| {
        let Key::Named(named) = key else {
            return false;
        };
        let step = KEY_STEP * step_factor(telar::modifiers());
        let Hsv { h, s, v } = keys.hsv.peek();
        let (s, v) = match named {
            NamedKey::ArrowRight => (s + step, v),
            NamedKey::ArrowLeft => (s - step, v),
            NamedKey::ArrowUp => (s, v + step),
            NamedKey::ArrowDown => (s, v - step),
            _ => return false,
        };
        keys.set_hsv(Hsv {
            h,
            s: s.clamp(0.0, 1.0),
            v: v.clamp(0.0, 1.0),
        });
        true
    })
    .on_drag(move |x, y| {
        let h = drag.hsv.peek().h;
        drag.set_hsv(Hsv {
            h,
            s: (x / width).clamp(0.0, 1.0),
            v: (1.0 - y / height).clamp(0.0, 1.0),
        });
    });
    Ok(box_item(area))
}

/// One channel along a track: where it stands as `0..=1`, how to set it, what its track is painted with, and what a reader is told.
struct Channel {
    read: Rc<dyn Fn() -> f32>,
    write: Rc<dyn Fn(f32)>,
    paint: Rc<dyn Fn(Rect) -> Paint>,
    checkered: bool,
    name: &'static str,
    /// The announced range: degrees for the hue, a percentage for the opacity.
    range: f64,
}

fn hue_slider(picker: &Picker, width: f32) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (read, write) = (picker.clone(), picker.clone());
    channel(
        Channel {
            read: Rc::new(move || read.hsv.get().h / 360.0),
            write: Rc::new(move |t| {
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
            checkered: false,
            name: strings::HUE,
            range: 360.0,
        },
        width,
    )
}

fn alpha_slider(picker: &Picker, width: f32) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (read, write, paint) = (picker.clone(), picker.clone(), picker.clone());
    channel(
        Channel {
            read: Rc::new(move || read.value.get().a),
            write: Rc::new(move |t| write.set_alpha(t)),
            paint: Rc::new(move |r: Rect| {
                let opaque = paint.value.get().with_alpha(1.0);
                Paint::Gradient(Gradient::linear(
                    Point::new(r.x, r.y),
                    Point::new(r.x + r.width, r.y),
                    &[(0.0, opaque.with_alpha(0.0)), (1.0, opaque)],
                ))
            }),
            checkered: true,
            name: strings::OPACITY,
            range: 100.0,
        },
        width,
    )
}

fn channel(channel: Channel, width: f32) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let Channel {
        read,
        write,
        paint,
        checkered,
        name,
        range,
    } = channel;
    let radius = TRACK_HEIGHT / 2.0;
    let mut layers: Vec<Box<dyn LayoutItem>> = Vec::new();
    if checkered {
        layers.push(checker(LayoutStyle::new().absolute_fill())?);
    }
    layers.push(box_item(StyledContainer::new(
        LayoutStyle::new().absolute_fill(),
        move |r| {
            RectStyle::default()
                .with_fill(paint(r))
                .with_border(Border::uniform(shared::border(), 1.0))
                .with_radius(BorderRadius::all(radius))
        },
        vec![],
    )?));
    let size = thumb_size();
    let at = read.clone();
    layers.push(box_item(
        StyledContainer::new(
            LayoutStyle::new().absolute().width(size).height(size),
            move |_r| {
                RectStyle::default()
                    .with_border(ring())
                    .with_radius(BorderRadius::all(size / 2.0))
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
                t * (width - size),
                (TRACK_HEIGHT - size) / 2.0,
            )
        })
        .a11y_hidden(),
    ));

    let (announced, keyed) = (read.clone(), read);
    let drag = write.clone();
    let track = StyledContainer::new(
        LayoutStyle::new().width(width).height(TRACK_HEIGHT),
        |_r| RectStyle::default(),
        layers,
    )?
    .control(Role::Slider)
    .a11y_label(move || strings::text(name))
    .valued(move || NumericValue {
        now: (f64::from(announced()) * range).round(),
        min: 0.0,
        max: range,
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
        write(next.clamp(0.0, 1.0));
        true
    })
    .on_drag(move |x, _| drag((x / width).clamp(0.0, 1.0)));
    Ok(box_item(track))
}

/// The squares a translucent colour is shown over, so how much shows through can be seen.
fn checker(style: LayoutStyle) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let canvas = Canvas::new(style, |rect| {
        let light = Color::rgba(1.0, 1.0, 1.0, 1.0);
        let dark = Color::rgba(0.8, 0.8, 0.82, 1.0);
        let columns = (rect.width / CHECKER_CELL).ceil() as usize;
        let rows = (rect.height / CHECKER_CELL).ceil() as usize;
        let mut cells = vec![RenderNode::rect(
            rect,
            RectStyle::default().with_fill(light),
        )];
        for row in 0..rows {
            for column in (row % 2..columns).step_by(2) {
                let x = column as f32 * CHECKER_CELL;
                let y = row as f32 * CHECKER_CELL;
                cells.push(RenderNode::rect(
                    Rect::new(
                        x,
                        y,
                        CHECKER_CELL.min(rect.width - x),
                        CHECKER_CELL.min(rect.height - y),
                    ),
                    RectStyle::default().with_fill(dark),
                ));
            }
        }
        RenderNode::group(cells)
    })?
    .a11y_hidden();
    Ok(box_item(canvas))
}

/// The colour as it stands, alpha and all, over the checker.
fn chip(value: RwSignal<Color>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let size = chip_size();
    let face = StyledContainer::new(
        LayoutStyle::new().absolute_fill(),
        move |_r| {
            RectStyle::default()
                .with_fill(value.get())
                .with_border(Border::uniform(shared::border(), 1.0))
                .with_radius(BorderRadius::all(shared::radius_sm()))
        },
        vec![],
    )?;
    let chip = Container::new(
        LayoutStyle::new().width(size).height(size).flex_shrink(0.0),
        vec![checker(LayoutStyle::new().absolute_fill())?, box_item(face)],
    )?
    .a11y_hidden();
    Ok(box_item(chip))
}

/// The colour as hex: rewritten whenever the colour moves, and applied on Enter or Tab when it parses. Escape, or text that does not parse, puts back the colour's own.
fn hex_field(picker: &Picker, width: f32, alpha: bool) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let value = picker.value;
    let text = signal(hex_of(value.peek(), alpha));
    effect(move || {
        let shown = hex_of(value.get(), alpha);
        text.set_if_changed(shown);
    });
    let apply: Rc<dyn Fn()> = {
        let picker = picker.clone();
        Rc::new(move || {
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
                Key::Named(NamedKey::Tab) => apply(),
                Key::Named(NamedKey::Escape) => text.set(hex_of(value.peek(), alpha)),
                _ => {}
            }
            false
        })
    };
    text_field(
        TextFieldProps::props()
            .value(text)
            .placeholder(Reactive::of(|| strings::text(strings::HEX)))
            .width(width)
            .on_submit(apply)
            .on_key(on_key)
            .build(),
        Children::default(),
    )
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
        Rc::new(move |index: u32| {
            if let Some(&color) = palette.get(index as usize) {
                picker.set_color(color);
            }
        })
    };
    swatches(
        SwatchesProps::props()
            .colors(palette)
            .names(names)
            .selected(selected)
            .size(SWATCH_SIZE)
            .on_select(choose)
            .build(),
        Children::default(),
    )
}

#[cfg(test)]
#[path = "color_picker_test.rs"]
mod tests;
