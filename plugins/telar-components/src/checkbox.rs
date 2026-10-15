//! [`checkbox`]: a box and its label, toggling a bound boolean.

use std::rc::Rc;

use telar::{
    AlignItems, Border, BorderRadius, Children, Color, JustifyContent, LayoutError, LayoutItem,
    LayoutStyle, Props, Reactive, RectStyle, RwSignal, ShapeStyle, StyledContainer,
    SurfaceAmendment, amend_surface, box_item, children, focus::Role, signal,
};

use crate::shared;

/// A checkbox's own tokens, which an application's theme supplies through a `#[theme(extension)]` field.
///
/// Every field left `None` follows the role named beside it, read when the checkbox paints, so a theme that moves the role still moves every checkbox it has not given a value of its own.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CheckboxTokens {
    /// Side of the box at the regular control size, scaled with the control size in force. `None` is [`metrics::icon_size`](crate::metrics::icon_size).
    pub size: Option<f32>,
    /// Corner radius of the box; the mark's is concentric with it. `None` is the theme's `radius_item`.
    pub radius: Option<f32>,
    /// Width of the box's frame. `None` is the theme's `border_width_strong`.
    pub border_width: Option<f32>,
}

impl CheckboxTokens {
    /// The tokens of the theme in force, subscribing the caller to it.
    pub fn current() -> Self {
        telar::use_theme_extension::<Self>()
    }

    pub fn size(&self) -> f32 {
        self.size
            .map_or_else(shared::icon_size, |size| size * telar::control_scale())
    }

    pub fn radius(&self) -> f32 {
        self.radius.unwrap_or_else(shared::radius_item)
    }

    pub fn border_width(&self) -> f32 {
        self.border_width
            .unwrap_or_else(shared::border_width_strong)
    }
}

/// How far the mark stands inside the box's edge.
fn mark_inset() -> f32 {
    shared::gap_sm()
}

fn box_style() -> LayoutStyle {
    let size = CheckboxTokens::current().size();
    LayoutStyle::row()
        .width(size)
        .height(size)
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER)
}

fn mark_style() -> LayoutStyle {
    let side = (CheckboxTokens::current().size() - mark_inset() * 2.0).max(0.0);
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
    /// Accent (the checked fill). `Color::TRANSPARENT` (the default) means "unset": fall back to the theme accent.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub color: Reactive<Color>,
    /// Fires with the new state on every toggle.
    #[props(some, default)]
    pub on_toggle: Option<Rc<dyn Fn(bool)>>,
    /// Amends the paint of the box — the checkbox's **principal surface** — checked and unchecked. See [`telar::SurfaceStyle`] for when a theme token or [`CheckboxTokens`] is the right instrument instead.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
}

/// A box and its label, toggling a bound boolean.
pub fn checkbox(
    props: CheckboxProps,
    _children: Children,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let CheckboxProps {
        checked,
        label,
        color,
        on_toggle,
        style,
    } = props;
    let checked = checked.unwrap_or_else(|| signal(false));

    // The mark only paints while checked, so toggling never reflows; its ink is read off the box's own fill, since a fixed white vanished on the near-white `primary` of a neutral palette.
    let mark_checked = checked;
    let mark_color = color.clone();
    let mark = StyledContainer::new(
        mark_style(),
        move |_r| {
            let fill = if mark_checked.get() {
                shared::ink_on(shared::resolve(&mark_color, shared::accent))
            } else {
                Color::TRANSPARENT
            };
            let radius = (CheckboxTokens::current().radius() - mark_inset()).max(0.0);
            RectStyle::default()
                .with_fill(fill)
                .with_radius(BorderRadius::all(radius))
        },
        vec![],
    )?
    .styled_by(mark_style);

    let box_checked = checked;
    let control = StyledContainer::new(
        box_style(),
        move |_r| {
            let tokens = CheckboxTokens::current();
            let (fill, frame) = if box_checked.get() {
                let accent = shared::resolve(&color, shared::accent);
                (accent, accent)
            } else {
                (shared::surface(), shared::border())
            };
            let rest = RectStyle::default()
                .with_fill(fill)
                .with_border(Border::uniform(frame, tokens.border_width()))
                .with_radius(BorderRadius::all(tokens.radius()));
            amend_surface(rest, &style)
        },
        children![mark],
    )?
    .styled_by(box_style);

    let toggle_checked = checked;
    let announced = checked;
    shared::labelled_control(
        box_item(control),
        label,
        Role::CheckBox,
        move || announced.get(),
        move || {
            let next = !toggle_checked.get();
            toggle_checked.set(next);
            if let Some(cb) = &on_toggle {
                cb(next);
            }
        },
    )
}

#[cfg(test)]
#[path = "checkbox_test.rs"]
mod tests;
