//! [`checkbox`]: a box and its label, toggling a bound boolean.

use telar::{
    AlignItems, Border, BorderRadius, Color, Handler, JustifyContent, LayoutError, LayoutItem,
    LayoutStyle, Props, Reactive, RectStyle, RwSignal, ShapeStyle, StyledContainer,
    SurfaceAmendment, amend_surface, box_item, children, focus::Role, signal,
};

use crate::shared;
use telar::ControlSize;

/// A checkbox's own tokens, which an application's theme supplies through a `#[theme(extension)]` field.
///
/// Every field left `None` follows the role named beside it, read when the checkbox paints, so a theme that moves the role still moves every checkbox it has not given a value of its own.
#[derive(Clone, Debug, Default, PartialEq, telar::ComponentTokens)]
pub struct CheckboxTokens {
    /// Side of the box at the regular size, scaled with the checkbox's own. `None` is [`metrics::icon_size`](crate::metrics::icon_size).
    #[token(or = shared::icon_size, scaled)]
    pub size: Option<f32>,
    /// Corner radius of the box; the mark's is concentric with it. `None` is the theme's `radius_item`.
    #[token(or = shared::radius_item)]
    pub radius: Option<f32>,
    /// Width of the box's frame. `None` is the theme's `border_width_strong`.
    #[token(or = shared::border_width_strong)]
    pub border_width: Option<f32>,
}

/// How far the mark stands inside the box's edge.
fn mark_inset(size: ControlSize) -> f32 {
    shared::gap_sm(size)
}

fn box_style(size: ControlSize) -> LayoutStyle {
    let side = CheckboxTokens::current().size(size);
    LayoutStyle::row()
        .width(side)
        .height(side)
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER)
}

fn mark_style(size: ControlSize) -> LayoutStyle {
    let side = (CheckboxTokens::current().size(size) - mark_inset(size) * 2.0).max(0.0);
    LayoutStyle::new().width(side).height(side)
}

/// A labelled checkbox: a box that fills with the accent and shows a mark while its bound `checked` signal is on; tapping the row toggles it (and fires `on_toggle`). High-level sugar over the primitives (`box` + `on_press` + a reactive fill); lives in `telar-components`, not the kernel, so an app can drop it. `checked` is `Option` so `Props` can derive `Default`: `None` is uncontrolled (the widget owns its own signal), `Some` is caller-bound.
#[derive(Props)]
pub struct CheckboxProps {
    /// Bound checked state. `None` (the default) is uncontrolled — the widget makes its own `signal(false)`.
    #[props(some, into, default)]
    pub checked: Option<RwSignal<bool>>,
    #[props(into, default)]
    pub label: Reactive<String>,
    /// How large the checkbox is drawn: its box, mark and label, and the control height its row stands at, as [`ControlSize`] works them out.
    #[props(default)]
    pub size: ControlSize,
    /// Accent (the checked fill). `None` (the default) is the theme accent.
    #[props(some, into, default)]
    pub color: Option<Reactive<Color>>,
    /// Fires with the new state on every toggle.
    #[props(some, default)]
    pub on_toggle: Option<Handler<fn(bool)>>,
    /// Amends the paint of the box — the checkbox's **principal surface** — checked and unchecked. See [`telar::SurfaceStyle`] for when a theme token or [`CheckboxTokens`] is the right instrument instead.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
}

/// A box and its label, toggling a bound boolean.
pub fn checkbox(props: CheckboxProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let CheckboxProps {
        checked,
        label,
        size,
        color,
        on_toggle,
        style,
    } = props;
    let checked = checked.unwrap_or_else(|| signal(false));

    // The mark only paints while checked, so toggling never reflows; its ink is read off the box's own fill, since a fixed white vanished on the near-white `primary` of a neutral palette.
    let mark_checked = checked;
    let mark_color = color.clone();
    let mark = StyledContainer::new(
        mark_style(size),
        move |_r| {
            let fill = if mark_checked.get() {
                shared::ink_on(shared::resolve(&mark_color, shared::accent))
            } else {
                Color::TRANSPARENT
            };
            let radius = (CheckboxTokens::current().radius() - mark_inset(size)).max(0.0);
            RectStyle::default()
                .with_fill(fill)
                .with_radius(BorderRadius::all(radius))
        },
        vec![],
    )?
    .styled_by(move || mark_style(size));

    let box_checked = checked;
    let control = StyledContainer::new(
        box_style(size),
        move |_r| {
            let tokens = CheckboxTokens::current();
            let (fill, frame) = if box_checked.get() {
                let accent = shared::resolve(&color, shared::accent);
                (accent, accent)
            } else {
                (shared::surface(), shared::control_border())
            };
            let rest = RectStyle::default()
                .with_fill(fill)
                .with_border(Border::uniform(frame, tokens.border_width()))
                .with_radius(BorderRadius::all(tokens.radius()));
            amend_surface(rest, &style)
        },
        children![mark],
    )?
    .styled_by(move || box_style(size));

    let toggle_checked = checked;
    let announced = checked;
    shared::labelled_control(
        size,
        box_item(control),
        label,
        Role::CheckBox,
        move || announced.get(),
        move || {
            let next = !toggle_checked.get();
            toggle_checked.set(next);
            if let Some(cb) = &on_toggle {
                cb.run(next);
            }
        },
    )
    .map(box_item)
}

#[cfg(test)]
#[path = "checkbox.test.rs"]
mod tests;
