//! [`toggle`]: a switch flipping a bound boolean.

use std::rc::Rc;

use telar::{
    AlignItems, BorderRadius, Children, Color, LayoutError, LayoutItem, LayoutStyle, Props,
    Reactive, RectStyle, RwSignal, ShapeStyle, StyledContainer, SurfaceAmendment, amend_surface,
    box_item, box_transform, focus::Role, signal,
};

use crate::shared;

/// A switch's own tokens, which an application's theme supplies through a `#[theme(extension)]` field.
///
/// Every length is at the regular control size and scaled with the control size in force, and every field left `None` follows the role named beside it, read when the switch paints. The track is sized from the knob: two knobs and an inset each side long, one knob and an inset each side tall, so the knob travels one diameter.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ToggleTokens {
    /// Diameter of the knob. `None` is [`metrics::icon_size`](crate::metrics::icon_size).
    pub knob: Option<f32>,
    /// Space between the knob and the track's edge. `None` is [`metrics::gap_xs`](crate::metrics::gap_xs).
    pub inset: Option<f32>,
    /// Corner radius of the track; the knob's follows it, less the inset. `None` is a pill.
    pub radius: Option<f32>,
}

impl ToggleTokens {
    /// The tokens of the theme in force, subscribing the caller to it.
    pub fn current() -> Self {
        telar::use_theme_extension::<Self>()
    }

    pub fn knob(&self) -> f32 {
        self.knob
            .map_or_else(shared::icon_size, |knob| knob * telar::control_scale())
    }

    pub fn inset(&self) -> f32 {
        self.inset
            .map_or_else(shared::gap_xs, |inset| inset * telar::control_scale())
    }

    /// The track's corner for a track laid out at `rect`.
    pub fn track_radius(&self, rect: telar::Rect) -> BorderRadius {
        self.radius
            .map_or_else(|| shared::pill(rect), BorderRadius::all)
    }

    /// The knob's corner for a knob laid out at `rect`: concentric with the track's.
    pub fn knob_radius(&self, rect: telar::Rect) -> BorderRadius {
        self.radius.map_or_else(
            || shared::pill(rect),
            |radius| BorderRadius::all((radius - self.inset()).max(0.0)),
        )
    }
}

fn knob_box() -> LayoutStyle {
    let knob = ToggleTokens::current().knob();
    LayoutStyle::new().width(knob).height(knob)
}

fn track_box() -> LayoutStyle {
    let tokens = ToggleTokens::current();
    let (knob, inset) = (tokens.knob(), tokens.inset());
    LayoutStyle::new()
        .flex_row()
        .width(knob * 2.0 + inset * 2.0)
        .height(knob + inset * 2.0)
        .padding_all(inset)
        .align_items(AlignItems::CENTER)
}

/// The track's fill: the accent while on, else the strongest of the theme's washes, so an off switch still reads as a track against the surface it sits on.
fn track_fill(on: bool, color: &Reactive<Color>) -> Color {
    if on {
        shared::resolve(color, shared::accent)
    } else {
        shared::pressed_wash()
    }
}

/// A labelled switch: a pill track whose knob sits at the start (off) or the end (on) and whose track fills with the accent while on; tapping the row toggles its bound `checked` signal (and fires `on_toggle`). High-level sugar over the primitives (`box` + `on_press` + a reactive fill + a `translate`); lives in `telar-components`, not the kernel. `checked` is `Option` so `Props` can derive `Default`: `None` is uncontrolled (the widget owns its own signal), `Some` is caller-bound.
#[derive(Props)]
pub struct ToggleProps {
    /// Bound on/off state. `None` (the default) is uncontrolled — the widget makes its own `signal(false)`.
    #[props(some, into, default)]
    pub checked: Option<RwSignal<bool>>,
    #[props(into, default)]
    pub label: Reactive<String>,
    /// Accent (the on-track fill). `Color::TRANSPARENT` (the default) means "unset": fall back to the theme accent.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub color: Reactive<Color>,
    /// Fires with the new state on every toggle.
    #[props(some, default)]
    pub on_toggle: Option<Rc<dyn Fn(bool)>>,
    /// Amends the paint of the track — the switch's **principal surface** — on and off. See [`telar::SurfaceStyle`] for when a theme token or [`ToggleTokens`] is the right instrument instead.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
}

/// A switch flipping a bound boolean.
pub fn toggle(props: ToggleProps, _children: Children) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let ToggleProps {
        checked,
        label,
        color,
        on_toggle,
        style,
    } = props;
    let checked = checked.unwrap_or_else(|| signal(false));

    let knob_on = checked;
    let knob_color = color.clone();
    let knob = StyledContainer::new(
        knob_box(),
        move |r| {
            let track = shared::over(track_fill(knob_on.get(), &knob_color), shared::surface());
            RectStyle::default()
                .with_fill(shared::thumb_on(track))
                .with_radius(ToggleTokens::current().knob_radius(r))
        },
        vec![],
    )?
    .styled_by(knob_box)
    .with_transform(move |r| {
        let travel = if knob_on.get() {
            ToggleTokens::current().knob()
        } else {
            0.0
        };
        box_transform(r, 0.0, 1.0, 1.0, travel, 0.0)
    });

    let track_on = checked;
    let track = StyledContainer::new(
        track_box(),
        move |r| {
            let rest = RectStyle::default()
                .with_fill(track_fill(track_on.get(), &color))
                .with_radius(ToggleTokens::current().track_radius(r));
            amend_surface(rest, &style)
        },
        vec![box_item(knob)],
    )?
    .styled_by(track_box);

    let toggle_on = checked;
    let announced = checked;
    shared::labelled_control(
        box_item(track),
        label,
        Role::Switch,
        move || announced.get(),
        move || {
            let next = !toggle_on.get();
            toggle_on.set(next);
            if let Some(cb) = &on_toggle {
                cb(next);
            }
        },
    )
}

#[cfg(test)]
#[path = "toggle_test.rs"]
mod tests;
