//! [`slider`]: a track and thumb dragged, or stepped with the arrow keys, over a range.

use std::ops::RangeInclusive;
use std::rc::Rc;
use telar::{
    Accessible, Border, Color, Handler, Insets, LayoutError, LayoutItem, LayoutStyle, Props,
    Reactive, RectStyle, RwSignal, ShapeStyle, StyledContainer, SurfaceAmendment, Transform,
    amend_surface, box_item, box_transform, children, focus::Role, signal,
};

use crate::shared;
use telar::ControlSize;

/// A slider's own tokens, which an application's theme supplies through a `#[theme(extension)]` field.
///
/// Every length is at the regular size and scaled with the slider's own, and every field left `None` follows the role named beside it, read when the slider paints.
#[derive(Clone, Debug, Default, PartialEq, telar::ComponentTokens)]
pub struct SliderTokens {
    /// Length of the track where what holds it leaves the room: shorter where it does not, and as its call places it (`width:100%`). `None` is 16 ems of the document's text, a progress bar's length.
    pub width: Option<f32>,
    /// Thickness of the track. `None` is [`metrics::spacing`](crate::metrics::spacing).
    #[token(or = shared::spacing, scaled)]
    pub track: Option<f32>,
    /// Diameter of the thumb. `None` is [`metrics::icon_size`](crate::metrics::icon_size).
    #[token(or = shared::icon_size, scaled)]
    pub thumb: Option<f32>,
    /// Width of the ring that parts the thumb from the fill under it. `None` is the theme's `border_width_strong`.
    #[token(or = shared::border_width_strong)]
    pub ring: Option<f32>,
}

const WIDTH_EMS: f32 = 16.0;

impl SliderTokens {
    /// The track's length in a slider of `size` where what holds it leaves the room.
    pub fn width(&self, size: ControlSize) -> f32 {
        shared::width_or_measure(size, self.width, WIDTH_EMS)
    }
}

fn track_height(size: ControlSize) -> f32 {
    SliderTokens::current().track(size)
}
fn thumb_size(size: ControlSize) -> f32 {
    SliderTokens::current().thumb(size)
}

fn thumb_box(size: ControlSize) -> LayoutStyle {
    LayoutStyle::new()
        .width(thumb_size(size))
        .height(thumb_size(size))
}
/// The track is the item the slider's call places unless a caption's column stands around it, and then takes that column's width.
fn track_box(size: ControlSize, captioned: bool) -> LayoutStyle {
    let track = LayoutStyle::new().height(track_height(size));
    match captioned {
        true => track,
        false => shared::preferred_width(track, SliderTokens::current().width(size)),
    }
}

/// How far the track's grab area reaches past its long sides, so the thin track and the thumb overhanging it are one target at least [`shared::MIN_TARGET`] tall.
fn grab_outset(size: ControlSize) -> Insets {
    let reach = ((shared::MIN_TARGET.max(thumb_size(size)) - track_height(size)) / 2.0).max(0.0);
    Insets::new(reach, 0.0, reach, 0.0)
}

/// A drag-driven control over a range: a rounded track, an accent fill up to `value`, and a thumb positioned by it. This is the canonical demo of the `on_drag` primitive (see `StyledContainer::on_drag`): the widget just encapsulates the press/move -> value mapping (and the fill/thumb repaint) an app would otherwise wire up by hand. High-level sugar over the primitives; lives in `telar-components`, not the kernel, so an app can drop it.
#[derive(Props)]
pub struct SliderProps {
    /// Bound value, reported in its range (out-of-range inputs are clamped on every drag report, not here). `None` (the default) is uncontrolled — the widget makes its own, starting at the start of its range.
    #[props(some, into, default)]
    pub value: Option<RwSignal<f32>>,
    /// Fill/thumb accent. `None` (the default) is the theme accent.
    #[props(some, into, default)]
    pub color: Option<Reactive<Color>>,
    /// Track (rail) colour; `None` (the default) is the theme's muted token.
    #[props(some, into, default)]
    pub track_color: Option<Reactive<Color>>,
    /// How large the slider is drawn: its track, thumb and caption, and the length it takes where what holds it leaves the room, as [`ControlSize`] works them out.
    #[props(default)]
    pub size: ControlSize,
    /// The values the slider reports, from its start at the left end of the track to its end at the right; `0.0..=1.0` by default. A range whose start is not below its end is an error when the slider is built.
    #[props(default = 0.0..=1.0)]
    pub range: RangeInclusive<f32>,
    /// The step a reported value snaps to and an arrow key moves by. `None` (the default) is continuous, the keys moving a twentieth of the range; a step that is not above zero is an error when the slider is built.
    #[props(some, default)]
    pub step: Option<f32>,
    /// A small caption stacked above the track; omitted entirely (no extra row) when empty.
    #[props(into, default)]
    pub label: Reactive<String>,
    /// Fires with the new value, inside the range, on every drag report (the press and each subsequent move).
    #[props(some, into, default)]
    pub on_change: Option<Handler<fn(f32)>>,
    /// Amends the paint of the track — the slider's **principal surface**. See [`telar::SurfaceStyle`] for when a theme token or [`SliderTokens`] is the right instrument instead.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
}

/// A track and thumb dragged, or stepped with the arrow keys, over a range.
pub fn slider(props: SliderProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let SliderProps {
        value,
        color,
        track_color,
        size,
        range,
        step,
        label,
        on_change,
        style,
    } = props;
    let (min, max) = shared::ordered("slider", range)?;
    if min == max {
        return Err(LayoutError::Engine(format!(
            "slider: its range holds the one value {min}, which leaves nothing to slide over"
        )));
    }
    let step = step
        .map(|step| shared::positive_step("slider", step))
        .transpose()?;
    let value = value.unwrap_or_else(|| signal(min));
    let captioned = shared::is_captioned(&label);
    let announced_value = value;
    let snap = move |v: f32| match step {
        Some(step) => {
            // Rounding can walk a boundary value just past min or max.
            let steps = ((v - min) / step).round();
            (min + steps * step).clamp(min, max)
        }
        None => v.clamp(min, max),
    };
    let commit: Rc<dyn Fn(f32) -> bool> = {
        let on_change = on_change.clone();
        Rc::new(move |next: f32| {
            let moved = next != value.peek();
            value.set(next);
            if let Some(cb) = &on_change {
                cb.run(next);
            }
            moved
        })
    };
    // One step per press, or a twentieth of the range when the caller named none — the granularity a continuous value has to invent for a keyboard.
    let stride = step.unwrap_or((max - min) / 20.0);
    let page = match step {
        Some(step) => (((max - min) / 10.0 / step).round().max(1.0)) * step,
        None => (max - min) / 10.0,
    };
    let shift_by = {
        let commit = Rc::clone(&commit);
        move |distance: f32| commit((value.peek() + distance).clamp(min, max))
    };
    let step_by = {
        let shift_by = shift_by.clone();
        move |steps: f32| shift_by(steps * stride)
    };

    // An `absolute_fill` child, so it exactly overlays the track, scaled horizontally from the left edge — cheaper than relaying out a narrower box on every drag move.
    let fill_value = value;
    let fill_color = color.clone();
    let fill = StyledContainer::new(
        LayoutStyle::new().absolute_fill(),
        move |r| {
            let fill = shared::resolve(&fill_color, shared::accent);
            RectStyle::default()
                .with_fill(fill)
                .with_radius(shared::pill(r))
        },
        vec![],
    )?
    .with_transform(move |r| {
        let t = ((fill_value.get() - min) / (max - min)).clamp(0.0, 1.0);
        // `box_transform` only pivots scale on the rect centre; a progress bar needs the left edge pinned, which needs the raw matrix.
        Some(Transform::scale_around(t, 1.0, r.x, r.y + r.height / 2.0))
    });
    // The fill lies over the whole track and exists before it, so its laid rect is the length the thumb travels and a drag is measured against.
    let track_rect = telar::track_layout(fill.layout_node()).expect("the fill node is registered");

    // A normal in-flow child, so it lands at the track's top-left; a translate then carries it to the value's position and re-centres it against the thinner track.
    let thumb_value = value;
    let thumb_color = color.clone();
    let thumb = StyledContainer::new(
        thumb_box(size),
        move |r| {
            let fill = shared::resolve(&thumb_color, shared::accent);
            RectStyle::default()
                .with_fill(fill)
                .with_border(Border::uniform(
                    shared::thumb_on(fill),
                    SliderTokens::current().ring(),
                ))
                .with_radius(shared::pill(r))
        },
        vec![],
    )?
    .styled_by(move || thumb_box(size))
    .with_transform(move |r| {
        let t = ((thumb_value.get() - min) / (max - min)).clamp(0.0, 1.0);
        let tx = t * (track_rect.get().width - thumb_size(size));
        let ty = (track_height(size) - thumb_size(size)) / 2.0;
        box_transform(r, 0.0, 1.0, 1.0, tx, ty)
    });

    let track = StyledContainer::new(
        track_box(size, captioned),
        move |r| {
            let fill = shared::resolve(&track_color, shared::muted);
            let rest = RectStyle::default()
                .with_fill(fill)
                .with_radius(shared::pill(r));
            amend_surface(rest, &style)
        },
        children![fill, thumb],
    )?
    .styled_by(move || track_box(size, captioned))
    .hit_outset(Reactive::of(move || grab_outset(size)))
    .control(Role::Slider)
    .a11y_label({
        let label = label.clone();
        move || label.get()
    })
    .valued({
        let value = announced_value;
        move || {
            telar::NumericValue::new(
                f64::from(min)..=f64::from(max),
                f64::from(value.get().clamp(min, max)),
            )
            .ok()
        }
    })
    .stepped({
        let step_by = step_by.clone();
        move |step| match step {
            telar::focus::Step::Increment => step_by(1.0),
            telar::focus::Step::Decrement => step_by(-1.0),
        }
    })
    .assigned({
        let commit = Rc::clone(&commit);
        move |assigned| {
            assigned
                .reading()
                .is_some_and(|reading| commit(snap(reading.now() as f32)))
        }
    })
    .on_focused_key({
        let commit = Rc::clone(&commit);
        move |key: &telar::Key| {
            use telar::NamedKey::*;
            match key {
                telar::Key::Named(ArrowRight | ArrowUp) => {
                    step_by(telar::step_factor(telar::modifiers()));
                }
                telar::Key::Named(ArrowLeft | ArrowDown) => {
                    step_by(-telar::step_factor(telar::modifiers()));
                }
                telar::Key::Named(PageUp) => {
                    shift_by(page);
                }
                telar::Key::Named(PageDown) => {
                    shift_by(-page);
                }
                telar::Key::Named(Home) => {
                    commit(min);
                }
                telar::Key::Named(End) => {
                    commit(max);
                }
                _ => {}
            }
        }
    })
    .on_drag(move |px, _py| {
        let t = (px / track_rect.peek().width).clamp(0.0, 1.0);
        commit(snap(min + t * (max - min)));
    });

    let width = Reactive::of(move || SliderTokens::current().width(size));
    shared::captioned(size, box_item(track), label, Some(width), false)
}

#[cfg(test)]
#[path = "slider.test.rs"]
mod tests;
