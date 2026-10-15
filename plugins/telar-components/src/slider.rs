//! [`slider`]: a track and thumb dragged, or stepped with the arrow keys, over a range.

use std::rc::Rc;
use telar::{
    Accessible, Border, Children, Color, Insets, LayoutError, LayoutItem, LayoutStyle, Props,
    Reactive, RectStyle, RwSignal, ShapeStyle, StyledContainer, SurfaceAmendment, Transform,
    amend_surface, box_item, box_transform, children, focus::Role, signal,
};

use crate::shared;

/// A slider's own tokens, which an application's theme supplies through a `#[theme(extension)]` field.
///
/// Every length is at the regular control size and scaled with the control size in force, and every field left `None` follows the role named beside it, read when the slider paints.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SliderTokens {
    /// Length of the track when the caller names no `width`. `None` is 16 ems of the document's text, a progress bar's length.
    pub width: Option<f32>,
    /// Thickness of the track. `None` is [`metrics::spacing`](crate::metrics::spacing).
    pub track: Option<f32>,
    /// Diameter of the thumb. `None` is [`metrics::icon_size`](crate::metrics::icon_size).
    pub thumb: Option<f32>,
    /// Width of the ring that parts the thumb from the fill under it. `None` is the theme's `border_width_strong`.
    pub ring: Option<f32>,
}

const WIDTH_EMS: f32 = 16.0;

impl SliderTokens {
    /// The tokens of the theme in force, subscribing the caller to it.
    pub fn current() -> Self {
        telar::use_theme_extension::<Self>()
    }

    pub fn width(&self) -> f32 {
        shared::width_or_measure(self.width, WIDTH_EMS)
    }

    pub fn track(&self) -> f32 {
        self.track
            .map_or_else(shared::spacing, |track| track * telar::control_scale())
    }

    pub fn thumb(&self) -> f32 {
        self.thumb
            .map_or_else(shared::icon_size, |thumb| thumb * telar::control_scale())
    }

    pub fn ring(&self) -> f32 {
        self.ring.unwrap_or_else(shared::border_width_strong)
    }
}

fn track_height() -> f32 {
    SliderTokens::current().track()
}
fn thumb_size() -> f32 {
    SliderTokens::current().thumb()
}

fn thumb_box() -> LayoutStyle {
    LayoutStyle::new().width(thumb_size()).height(thumb_size())
}
fn track_box(width: f32) -> LayoutStyle {
    LayoutStyle::new().width(width).height(track_height())
}

/// How far the track's grab area reaches past its long sides, so the thin track and the thumb overhanging it are one target at least [`shared::MIN_TARGET`] tall.
fn grab_outset() -> Insets {
    let reach = ((shared::MIN_TARGET.max(thumb_size()) - track_height()) / 2.0).max(0.0);
    Insets::new(reach, 0.0, reach, 0.0)
}

/// A drag-driven `min..=max` control: a rounded track, an accent fill up to `value`, and a thumb positioned by it. This is the canonical demo of the `on_drag` primitive (see `StyledContainer::on_drag`): the widget just encapsulates the press/move -> value mapping (and the fill/thumb repaint) an app would otherwise wire up by hand. High-level sugar over the primitives; lives in `telar-components`, not the kernel, so an app can drop it. `value` is `Option` so `Props` can derive `Default`: `None` is uncontrolled (the widget owns its own signal), `Some` is caller-bound.
#[derive(Props)]
pub struct SliderProps {
    /// Bound value, reported in `min..=max` (out-of-range inputs are clamped on every drag report, not here). `None` (the default) is uncontrolled — the widget makes its own `signal(min)`.
    #[props(some, into, default)]
    pub value: Option<RwSignal<f32>>,
    /// Fill/thumb accent. `Color::TRANSPARENT` (the default) means "unset": fall back to the theme accent.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub color: Reactive<Color>,
    /// Track (rail) colour; `Color::TRANSPARENT` (the default) means "unset": fall back to the theme's muted token.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub track_color: Reactive<Color>,
    /// Track width in px. `0.0` (the default) means "unset": the [`SliderTokens`] width at the control size in force.
    #[props(default)]
    pub width: f32,
    /// Lower bound of the reported value. Default `0.0`.
    #[props(default)]
    pub min: f32,
    /// Upper bound of the reported value. Default `1.0`. If `max <= min` (the degenerate/unset state), the slider falls back to `min=0.0, max=1.0` rather than dividing by zero.
    #[props(default)]
    pub max: f32,
    /// Quantization step applied to the reported value. `0.0` (the default) means "unset" — continuous, no snapping, following the same `0.0 == unset` sentinel convention as `width`.
    #[props(default)]
    pub step: f32,
    /// A small caption stacked above the track; omitted entirely (no extra row) when empty.
    #[props(into, default)]
    pub label: Reactive<String>,
    /// Fires with the new `min..=max` value on every drag report (the press and each subsequent move).
    #[props(some, default)]
    pub on_change: Option<Rc<dyn Fn(f32)>>,
    /// Amends the paint of the track — the slider's **principal surface**. See [`telar::SurfaceStyle`] for when a theme token or [`SliderTokens`] is the right instrument instead.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
}

/// A track and thumb dragged, or stepped with the arrow keys, over a range.
pub fn slider(props: SliderProps, _children: Children) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let SliderProps {
        value,
        color,
        track_color,
        width,
        min,
        max,
        step,
        label,
        on_change,
        style,
    } = props;
    let (min, max) = if max <= min { (0.0, 1.0) } else { (min, max) };
    let value = value.unwrap_or_else(|| signal(min));
    let width = if width > 0.0 {
        width
    } else {
        SliderTokens::current().width()
    };
    let key_on_change = on_change.clone();
    let announced_value = value;
    let commit_value = value;

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
        Some(Transform::scale_around(t, 1.0, r.x, r.y + r.height / 2.0).to_array())
    });

    // A normal in-flow child, so it lands at the track's top-left; a translate then carries it to the value's position and re-centres it against the thinner track.
    let thumb_value = value;
    let thumb_color = color.clone();
    let thumb = StyledContainer::new(
        thumb_box(),
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
    .styled_by(thumb_box)
    .with_transform(move |r| {
        let t = ((thumb_value.get() - min) / (max - min)).clamp(0.0, 1.0);
        let tx = t * (width - thumb_size());
        let ty = (track_height() - thumb_size()) / 2.0;
        box_transform(r, 0.0, 1.0, 1.0, tx, ty)
    });

    let track = StyledContainer::new(
        track_box(width),
        move |r| {
            let fill = shared::resolve(&track_color, shared::muted);
            let rest = RectStyle::default()
                .with_fill(fill)
                .with_radius(shared::pill(r));
            amend_surface(rest, &style)
        },
        children![fill, thumb],
    )?
    .styled_by(move || track_box(width))
    .hit_outset(Reactive::of(grab_outset))
    // A slider you can reach but not move is not operable. One step per press, or a twentieth of the range when the caller named none — the granularity a continuous value has to invent for a keyboard.
    .control(Role::Slider)
    .a11y_label({
        let label = label.clone();
        move || label.get()
    })
    .valued({
        let value = announced_value;
        move || telar::NumericValue {
            now: value.get() as f64,
            min: min as f64,
            max: max as f64,
        }
    })
    .on_focused_key({
        let value = commit_value;
        let on_change = key_on_change.clone();
        move |key: &telar::Key| {
            let delta = match key {
                telar::Key::Named(telar::NamedKey::ArrowRight)
                | telar::Key::Named(telar::NamedKey::ArrowUp) => 1.0,
                telar::Key::Named(telar::NamedKey::ArrowLeft)
                | telar::Key::Named(telar::NamedKey::ArrowDown) => -1.0,
                _ => return,
            };
            let stride = if step > 0.0 { step } else { (max - min) / 20.0 }
                * telar::step_factor(telar::modifiers());
            let next = (value.peek() + delta * stride).clamp(min, max);
            value.set(next);
            if let Some(cb) = &on_change {
                cb(next);
            }
        }
    })
    .on_drag(move |px, _py| {
        let t = (px / width).clamp(0.0, 1.0);
        let mut v = min + t * (max - min);
        if step > 0.0 {
            // Rounding can walk a boundary value just past min or max.
            let steps = ((v - min) / step).round();
            v = (min + steps * step).clamp(min, max);
        }
        value.set(v);
        if let Some(cb) = &on_change {
            cb(v);
        }
    });

    shared::captioned(box_item(track), label, width)
}

#[cfg(test)]
#[path = "slider_test.rs"]
mod tests;
