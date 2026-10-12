//! [`radio`]: one option of a group, selecting into a shared bound value.

use std::rc::Rc;

use telar::{
    AlignItems, Border, Children, Color, JustifyContent, LayoutError, LayoutItem, LayoutStyle,
    Props, Reactive, RectStyle, RwSignal, ShapeStyle, StyledContainer, SurfaceAmendment,
    amend_surface, box_item, focus::Role, signal,
};

use crate::shared;

/// A radio button's own tokens, which an application's theme supplies through a `#[theme(extension)]` field.
///
/// Every field left `None` follows the role named beside it, read when the radio paints. The ring and its dot are pills at whatever size they are laid out.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RadioTokens {
    /// Diameter of the ring at the regular control size, scaled with the control size in force. `None` is [`metrics::icon_size`](crate::metrics::icon_size).
    pub size: Option<f32>,
    /// Width of the ring. `None` is the theme's `border_width_strong`.
    pub border_width: Option<f32>,
}

impl RadioTokens {
    /// The tokens of the theme in force, subscribing the caller to it.
    pub fn current() -> Self {
        telar::use_theme_extension::<Self>()
    }

    pub fn size(&self) -> f32 {
        self.size
            .map_or_else(shared::icon_size, |size| size * telar::control_scale())
    }

    pub fn border_width(&self) -> f32 {
        self.border_width
            .unwrap_or_else(shared::border_width_strong)
    }
}

fn ring_style() -> LayoutStyle {
    let size = RadioTokens::current().size();
    LayoutStyle::new()
        .flex_row()
        .width(size)
        .height(size)
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER)
}

/// The dot stands inside the ring by the step between the parts of one control, as a checkbox's mark does inside its box.
fn dot_style() -> LayoutStyle {
    let side = (RadioTokens::current().size() - shared::gap_sm() * 2.0).max(0.0);
    LayoutStyle::new().width(side).height(side)
}

/// One radio button in a group: a ring that fills its centre dot (and accents its border) while the bound `selected` signal equals this button's `value`; tapping the row sets `selected` to `value` (and fires `on_select`). A radio *group* is several `radio`s sharing one `selected` signal with different `value`s. High-level sugar over the primitives (`box` + `on_press` + a reactive fill); lives in `telar-components`, not the kernel. `selected` is `Option` so `Props` can derive `Default`: `None` is uncontrolled (the widget owns its own signal, so it never matches — a lone default radio), `Some` is the shared group signal.
#[derive(Props)]
pub struct RadioProps {
    /// The group's bound selection. `None` (the default) is uncontrolled — the widget makes its own `signal(0)`.
    #[props(some, into, default)]
    pub selected: Option<RwSignal<u32>>,
    /// This button's value: it is selected when `selected` equals it, and a tap sets `selected` to it.
    #[props(default)]
    pub value: u32,
    #[props(into, default)]
    pub label: Reactive<String>,
    /// Accent (the selected dot and border). `Color::TRANSPARENT` (the default) means "unset": fall back to the theme accent.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub color: Reactive<Color>,
    /// Fires with this button's `value` when it becomes selected.
    #[props(some, default)]
    pub on_select: Option<Rc<dyn Fn(u32)>>,
    /// Amends the paint of the ring — the radio's **principal surface** — selected and not. See [`telar::SurfaceStyle`] for when a theme token or [`RadioTokens`] is the right instrument instead.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
}

/// One option of a group, selecting into a shared bound value.
pub fn radio(props: RadioProps, _children: Children) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let RadioProps {
        selected,
        value,
        label,
        color,
        on_select,
        style,
    } = props;
    let selected = selected.unwrap_or_else(|| signal(0u32));

    let dot_selected = selected;
    let dot_color = color.clone();
    let dot = StyledContainer::new(
        dot_style(),
        move |r| {
            let fill = if dot_selected.get() == value {
                shared::resolve(&dot_color, shared::accent)
            } else {
                Color::TRANSPARENT
            };
            RectStyle::default()
                .with_fill(fill)
                .with_radius(shared::pill(r))
        },
        vec![],
    )?
    .styled_by(dot_style);

    let ring_selected = selected;
    let ring = StyledContainer::new(
        ring_style(),
        move |r| {
            let stroke = if ring_selected.get() == value {
                shared::resolve(&color, shared::accent)
            } else {
                shared::border()
            };
            let rest = RectStyle::default()
                .with_fill(shared::surface())
                .with_border(Border::uniform(
                    stroke,
                    RadioTokens::current().border_width(),
                ))
                .with_radius(shared::pill(r));
            amend_surface(rest, &style)
        },
        vec![box_item(dot)],
    )?
    .styled_by(ring_style);

    let select = selected;
    let announced = selected;
    shared::labelled_control(
        box_item(ring),
        label,
        Role::Radio,
        move || announced.get() == value,
        move || {
            select.set(value);
            if let Some(cb) = &on_select {
                cb(value);
            }
        },
    )
}

#[cfg(test)]
#[path = "radio_test.rs"]
mod tests;
