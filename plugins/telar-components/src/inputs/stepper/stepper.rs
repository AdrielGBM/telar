//! [`stepper`]: a value between a minus and a plus button, clamped to its range.

use std::ops::RangeInclusive;
use std::rc::Rc;
use telar::{
    Accessible, AlignItems, BorderRadius, Color, Container, JustifyContent, LayoutError,
    LayoutItem, LayoutStyle, Props, Reactive, RectStyle, RwSignal, ShapeStyle, StyledContainer,
    SurfaceAmendment, Text, amend_surface, box_item, children, focus::Role, signal,
};

use crate::shared;
use crate::strings;
use telar::ControlSize;

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

fn row_box(size: ControlSize) -> LayoutStyle {
    LayoutStyle::row()
        .align_items(AlignItems::CENTER)
        .gap(shared::gap_md(size))
}
fn button_box(size: ControlSize) -> LayoutStyle {
    LayoutStyle::row()
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER)
        .width(shared::control_height(size))
        .height(shared::control_height(size))
}

/// A numeric stepper: `[−]  value  [+]`. High-level sugar over `button`-style pressable boxes (see `button.rs`) plus a reactive `Text::new` readout; lives in `telar-components`, not the kernel. `None` for `value` is uncontrolled: the widget owns a signal starting at the start of its range.
#[derive(Props)]
pub struct StepperProps {
    /// Bound value. `None` (the default) is uncontrolled — the widget makes its own, starting at the start of its range.
    #[props(some, into, default)]
    pub value: Option<RwSignal<f32>>,
    /// The values the stepper keeps to, its start no further than its end: from zero up, unbounded, by default. A range whose start lies past its end is an error when the stepper is built.
    #[props(default = 0.0..=f32::INFINITY)]
    pub range: RangeInclusive<f32>,
    /// What one press adds or takes away. One by default; a step that is not above zero is an error when the stepper is built.
    #[props(default = 1.0)]
    pub step: f32,
    /// How large the stepper is drawn: its buttons, gaps and value, as [`ControlSize`] works them out.
    #[props(default)]
    pub size: ControlSize,
    /// − / + fill. `None` (the default) is the theme's primary token.
    #[props(some, into, default)]
    pub color: Option<Reactive<Color>>,
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
        range,
        step,
        size,
        color,
        on_change,
        style,
    } = props;
    let (min, max) = shared::ordered("stepper", range)?;
    let step = shared::positive_step("stepper", step)?;
    // Uncontrolled: own the value so the stepper still works when the caller binds no signal.
    let value = value.unwrap_or_else(|| signal(min));
    let paint = Paint { color, style };

    let minus = stepper_button(size, "−", strings::DECREASE, paint.clone(), {
        let on_change = on_change.clone();
        move || {
            let v = (value.get() - step).clamp(min, max);
            value.set(v);
            if let Some(cb) = &on_change {
                cb(v);
            }
        }
    })?;

    let plus = stepper_button(size, "+", strings::INCREASE, paint, {
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
        move |t| shared::control_text(size, t, 1.0),
    )?;

    let row = Container::new(row_box(size), children![minus, display, plus])?
        .styled_by(move || row_box(size));
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
    color: Option<Reactive<Color>>,
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
    size: ControlSize,
    glyph: &'static str,
    name: &'static str,
    paint: Paint,
    on_press: impl Fn() + 'static,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let ink = paint.clone();
    let glyph_widget = Text::declaring(
        move || glyph.to_string(),
        LayoutStyle::new(),
        move |t| shared::control_text(size, t, 1.0).with_color(shared::ink_on(ink.fill())),
    )?;
    let (rest, hover, press) = (paint.clone(), paint.clone(), paint);
    let container = StyledContainer::new(
        button_box(size),
        move |_r| rest.surface(State::Rest),
        children![glyph_widget],
    )?
    .styled_by(move || button_box(size))
    .hover_style(move |_r| hover.surface(State::Hovered))
    .active_style(move |_r| press.surface(State::Pressed))
    .control(Role::Button)
    .a11y_label(move || strings::text(name))
    .on_press(on_press);
    Ok(box_item(container))
}

#[cfg(test)]
#[path = "stepper.test.rs"]
mod tests;
