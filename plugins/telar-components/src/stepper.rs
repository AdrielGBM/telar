//! [`stepper`]: a value between a minus and a plus button, clamped to its range.

use std::rc::Rc;
use telar::{
    Accessible, AlignItems, BorderRadius, Color, Container, JustifyContent, LayoutError,
    LayoutItem, LayoutStyle, Props, Reactive, RectStyle, RwSignal, ShapeStyle, StyledContainer,
    SurfaceAmendment, Text, amend_surface, box_item, children, focus::Role, signal,
};

use crate::shared;
use crate::strings;

/// A stepper's own tokens, which an application's theme supplies through a `#[theme(extension)]` field.
///
/// Every field left `None` follows the theme role named beside it, read when the stepper paints, so a theme that moves the role still moves every stepper it has not given a value of its own.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StepperTokens {
    /// Corner radius of the − and + buttons. `None` is the theme's `radius_control`.
    pub radius: Option<f32>,
}

impl StepperTokens {
    /// The tokens of the theme in force, subscribing the caller to it.
    pub fn current() -> Self {
        telar::use_theme_extension::<Self>()
    }

    pub fn radius(&self) -> f32 {
        self.radius.unwrap_or_else(shared::radius_control)
    }
}

fn row_box() -> LayoutStyle {
    LayoutStyle::row()
        .align_items(AlignItems::CENTER)
        .gap(shared::gap_md())
}
fn button_box() -> LayoutStyle {
    LayoutStyle::row()
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER)
        .width(shared::control_height())
        .height(shared::control_height())
}

/// A numeric stepper: `[−]  value  [+]`. High-level sugar over `button`-style pressable boxes (see `button.rs`) plus a reactive `Text::new` readout; lives in `telar-components`, not the kernel. `value` is `Option` so `Props` can derive `Default`: `None` is uncontrolled (the widget owns its own `signal(min)`).
#[derive(Props)]
pub struct StepperProps {
    /// Bound value. `None` (the default) is uncontrolled — the widget makes its own `signal(min)`.
    #[props(some, into, default)]
    pub value: Option<RwSignal<f32>>,
    /// Lower bound. Default `0.0`.
    #[props(default)]
    pub min: f32,
    /// Upper bound. `0.0` (the default) means "unset". Following the same `0.0 == unset` sentinel convention as `slider`'s `width`/`step`: an unset (or degenerate, `max <= min`) upper bound falls back to `f32::INFINITY` rather than the historical two-arg default, so an unset max never pins the value to `min` (a fixed fallback range would clamp any caller-supplied starting value above it back down).
    #[props(default)]
    pub max: f32,
    /// Increment applied per press. `0.0` (the default) means "unset" — use `1.0`.
    #[props(default)]
    pub step: f32,
    /// − / + fill. `Color::TRANSPARENT` (the default) means "unset": fall back to the theme's primary token.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub color: Reactive<Color>,
    /// Fires with the new (already clamped) value on every − / + press.
    #[props(some, default)]
    pub on_change: Option<Rc<dyn Fn(f32)>>,
    /// Amends the paint of the − and + buttons — the stepper's **principal surface** — at rest, hovered and pressed. See [`telar::SurfaceStyle`] for when a theme token or [`StepperTokens`] is the right instrument instead.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
}

/// A value between a minus and a plus button, clamped to its range.
pub fn stepper(props: StepperProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let StepperProps {
        value,
        min,
        max,
        step,
        color,
        on_change,
        style,
    } = props;
    let max = if max <= min { f32::INFINITY } else { max };
    let step = if step == 0.0 { 1.0 } else { step };
    // Uncontrolled: own the value so the stepper still works when the caller binds no signal.
    let value = value.unwrap_or_else(|| signal(min));
    let paint = Paint { color, style };

    let minus = stepper_button("−", strings::DECREASE, paint.clone(), {
        let on_change = on_change.clone();
        move || {
            let v = (value.get() - step).clamp(min, max);
            value.set(v);
            if let Some(cb) = &on_change {
                cb(v);
            }
        }
    })?;

    let plus = stepper_button("+", strings::INCREASE, paint, {
        let on_change = on_change.clone();
        move || {
            let v = (value.get() + step).clamp(min, max);
            value.set(v);
            if let Some(cb) = &on_change {
                cb(v);
            }
        }
    })?;

    let display_value = value;
    let display = Text::declaring(
        move || {
            let v = display_value.get();
            if v.fract() == 0.0 {
                format!("{}", v as i64)
            } else {
                format!("{v}")
            }
        },
        LayoutStyle::new(),
        |t| shared::control_text(t, 1.0),
    )?;

    let row = Container::new(row_box(), children![minus, display, plus])?.styled_by(row_box);
    Ok(box_item(row))
}

/// Which of its paints a − / + button is showing.
#[derive(Clone, Copy)]
enum State {
    Rest,
    Hovered,
    Pressed,
}

/// What both buttons paint with: the accent they fill with, and the caller's amendment.
#[derive(Clone)]
struct Paint {
    color: Reactive<Color>,
    style: Option<SurfaceAmendment>,
}

impl Paint {
    /// The caller's `color` when set, else the theme's `primary`.
    fn fill(&self) -> Color {
        shared::resolve(&self.color, shared::accent)
    }

    /// A button's paint in `state`: filled, and darkened under the pointer like any filled control.
    fn surface(&self, state: State) -> RectStyle {
        let fill = self.fill();
        let rect = RectStyle::default()
            .with_radius(BorderRadius::all(StepperTokens::current().radius()))
            .with_fill(match state {
                State::Rest => fill,
                State::Hovered => shared::hovered_fill(fill),
                State::Pressed => shared::pressed_fill(fill),
            });
        amend_surface(rect, &self.style)
    }
}

/// A square pressable box one control tall with a centred glyph, filled like a solid `button`.
fn stepper_button(
    glyph: &'static str,
    name: &'static str,
    paint: Paint,
    on_press: impl Fn() + 'static,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let ink = paint.clone();
    let glyph_widget = Text::declaring(
        move || glyph.to_string(),
        LayoutStyle::new(),
        move |t| shared::control_text(t, 1.0).with_color(shared::ink_on(ink.fill())),
    )?;
    let (rest, hover, press) = (paint.clone(), paint.clone(), paint);
    let container = StyledContainer::new(
        button_box(),
        move |_r| rest.surface(State::Rest),
        children![glyph_widget],
    )?
    .styled_by(button_box)
    .hover_style(move |_r| hover.surface(State::Hovered))
    .active_style(move |_r| press.surface(State::Pressed))
    .control(Role::Button)
    .a11y_label(move || strings::text(name))
    .on_press(on_press);
    Ok(box_item(container))
}

#[cfg(test)]
#[path = "stepper_test.rs"]
mod tests;
