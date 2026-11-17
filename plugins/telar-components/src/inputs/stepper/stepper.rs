//! [`stepper`]: a value between a minus and a plus button, clamped to its range, that a reader and the arrow keys step as one control.

use std::ops::RangeInclusive;
use std::rc::Rc;
use telar::{
    Accessible, AlignItems, BorderRadius, Color, Handler, JustifyContent, Key, LayoutError,
    LayoutItem, LayoutStyle, NamedKey, NumericValue, Props, Reactive, RectStyle, RwSignal,
    ShapeStyle, StyledContainer, SurfaceAmendment, Text, amend_surface, box_item, children,
    focus::{Role, Step},
    signal,
};

use crate::shared;
use telar::ControlSize;

/// A stepper's own tokens, which an application's theme supplies through a `#[theme(extension)]` field.
///
/// Every field left `None` follows the theme role named beside it, read when the stepper paints, so a theme that moves the role still moves every stepper it has not given a value of its own.
#[derive(Clone, Debug, Default, PartialEq, telar::ComponentTokens)]
pub struct StepperTokens {
    /// Corner radius of the − and + buttons. `None` is the theme's `radius_control`.
    #[token(or = shared::radius_control)]
    pub radius: Option<f32>,
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

/// A numeric stepper: `[−]  value  [+]`, one spin button a reader steps or sets and the arrow keys move while it holds focus; the − and + boxes are there for the pointer. Name it with `a11y_label`, since what it counts is said by whatever stands beside it. `None` for `value` is uncontrolled: the widget owns a signal starting at the start of its range.
#[derive(Props)]
pub struct StepperProps {
    /// Bound value. `None` (the default) is uncontrolled — the widget makes its own, starting at the start of its range.
    #[props(some, into, default)]
    pub value: Option<RwSignal<f32>>,
    /// The values the stepper keeps to, its start no further than its end: from zero up, unbounded, by default. A range whose start lies past its end is an error when the stepper is built.
    #[props(default = 0.0..=f32::INFINITY)]
    pub range: RangeInclusive<f32>,
    /// What one press, arrow key or reader's step adds or takes away, and the spacing a value a reader sets is rounded to, counted from the start of the range. One by default; a step that is not above zero is an error when the stepper is built.
    #[props(default = 1.0)]
    pub step: f32,
    /// How large the stepper is drawn: its buttons, gaps and value, as [`ControlSize`] works them out.
    #[props(default)]
    pub size: ControlSize,
    /// − / + fill. `None` (the default) is the theme's primary token.
    #[props(some, into, default)]
    pub color: Option<Reactive<Color>>,
    /// Fires with the new (already clamped) value whenever a press, an arrow key or a reader moves it.
    #[props(some, default)]
    pub on_change: Option<Handler<fn(f32)>>,
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

    let commit: Rc<dyn Fn(f32) -> bool> = Rc::new(move |next: f32| {
        let next = next.clamp(min, max);
        if next == value.peek() {
            return false;
        }
        value.set(next);
        if let Some(cb) = &on_change {
            cb.run(next);
        }
        true
    });
    let step_by = {
        let commit = Rc::clone(&commit);
        move |direction: Step| match direction {
            Step::Increment => commit(value.peek() + step),
            Step::Decrement => commit(value.peek() - step),
        }
    };

    let minus = stepper_button(size, "−", paint.clone(), {
        let step_by = step_by.clone();
        move || {
            step_by(Step::Decrement);
        }
    })?;
    let plus = stepper_button(size, "+", paint, {
        let step_by = step_by.clone();
        move || {
            step_by(Step::Increment);
        }
    })?;

    let display = Text::declaring(
        move || readout(value.get()),
        LayoutStyle::new(),
        move |t| shared::control_text(size, t, 1.0),
    )?;

    let origin = if min.is_finite() { min } else { 0.0 };
    let spin = StyledContainer::new(
        row_box(size),
        |_r| RectStyle::default().with_radius(BorderRadius::all(StepperTokens::current().radius())),
        children![minus, display, plus],
    )?
    .styled_by(move || row_box(size))
    .control(Role::SpinButton)
    .valued(move || NumericValue::new(min as f64..=max as f64, value.get() as f64).ok())
    .stepped(step_by.clone())
    .assigned(move |assigned| {
        assigned.reading().is_some_and(|reading| {
            let steps = ((reading.now() as f32 - origin) / step).round();
            commit(origin + steps * step);
            true
        })
    })
    .on_focused_key(move |key: &Key| match key {
        Key::Named(NamedKey::ArrowUp | NamedKey::ArrowRight) => {
            step_by(Step::Increment);
            true
        }
        Key::Named(NamedKey::ArrowDown | NamedKey::ArrowLeft) => {
            step_by(Step::Decrement);
            true
        }
        _ => false,
    });
    Ok(box_item(spin))
}

/// How the readout draws `value`: a whole number without a fraction.
fn readout(value: f32) -> String {
    if value.fract() == 0.0 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
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

/// A square pressable box one control tall with a centred glyph, filled like a solid `button`. Hidden from readers and out of the Tab order: the spin button around it steps the same value.
fn stepper_button(
    size: ControlSize,
    glyph: &'static str,
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
    .on_press(on_press)
    .a11y_hidden();
    Ok(box_item(container))
}

#[cfg(test)]
#[path = "stepper.test.rs"]
mod tests;
