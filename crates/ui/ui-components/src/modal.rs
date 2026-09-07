//! [`modal`]: a dialog portaled over the page, dismissed by its scrim, its Close or Escape.

use layout_core::{AlignItems, JustifyContent, LayoutError, LayoutStyle};
use reactive_core::{Reactive, RwSignal};
use renderer_core::{Border, BorderRadius, Color, RectStyle, ShapeStyle};
use std::rc::Rc;
use telar_macros::Props;
#[cfg(test)]
use ui_core::Slots;
use ui_core::focus::Role;
use ui_core::{Children, Container, LayoutItem, StyledContainer, Text, box_item};

use crate::heading::heading_style;
use crate::scrim;
use crate::shared;

/// Muted tone for the Close affordance.
fn dialog_radius() -> f32 {
    shared::radius() * 3.0
}
fn dialog_pad() -> f32 {
    shared::spacing() * 3.0
}
fn dialog_gap() -> f32 {
    shared::spacing() * 1.5
}
/// The close glyph's share of the text around it.
const CLOSE_RATIO: f32 = 0.93;

fn header_row() -> LayoutStyle {
    LayoutStyle::new()
        .flex_row()
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::SPACE_BETWEEN)
        .gap(dialog_gap())
}
fn card() -> LayoutStyle {
    LayoutStyle::new()
        .flex_column()
        .gap(dialog_gap())
        // So the dialog never collapses to its min-content width: the header and body leaves are unmeasured, with 0 intrinsic width, which would shrink the card to a strip.
        .min_width(320.0)
        .max_width(440.0)
        .padding_all(dialog_pad())
}
fn backdrop() -> LayoutStyle {
    LayoutStyle::new()
        .flex_column()
        .flex_grow(1.0)
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER)
        .padding_all(dialog_pad())
}

/// A centred dialog over a dimming scrim. When `open` is true it portals an `Overlay` (a top-layer layer that escapes clipping and blocks clicks behind it) with a translucent scrim and, centred on it, an opaque surface card holding the `title`, the slot children (the dialog body) and a Close affordance; when false it collapses to a 0-size node. Tapping the scrim (or Close) dismisses; tapping the card itself does not. High-level sugar built on the `overlay` primitive; lives in `ui-components`, not the kernel.
#[derive(Props)]
pub struct ModalProps {
    /// Bound open/close state. `None` (the default) never opens (no signal to read); `Some` drives the modal.
    #[props(some, into, default)]
    pub open: Option<RwSignal<bool>>,
    /// Names this dialog, so anything can open it with `open_overlay(id)` without holding its signal. `""` (the default) leaves it unnamed. Ignored when `open` is bound — an explicit signal wins, so the two forms never disagree about which state is authoritative.
    #[props(default = "")]
    pub id: &'static str,
    #[props(into, default)]
    pub title: Reactive<String>,
    /// Runs after the modal sets `open = false` (scrim tap or Close), so a caller can react to dismissal.
    #[props(some, default)]
    pub on_close: Option<Rc<dyn Fn()>>,
    /// Dialog surface colour. `Color::TRANSPARENT` (the default) means "unset" -> the theme's `surface`. A closure (re-read every frame) so a theme token or `$signal` colour re-colours live, like `button`'s `fill`.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub color: Reactive<Color>,
}

/// A dialog portaled over the page, dismissed by its scrim, its Close or Escape.
pub fn modal(props: ModalProps, children: Children) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let mut slots = children.build()?;
    let ModalProps {
        open,
        id,
        title,
        on_close,
        color,
    } = props;
    let body = slots.take_default();
    let open = shared::resolve_open(open, id);

    scrim::scrim_overlay(open, on_close, move |dismiss| {
        build_open_modal(title, body, color, dismiss)
    })
}

/// Builds the scrim + centred opaque card for the open state: scrim (dims + dismisses) > centred card (title row with Close, then the body). The card swallows its own taps so a click inside it never dismisses.
fn build_open_modal(
    title: Reactive<String>,
    body: Vec<Box<dyn LayoutItem>>,
    color: Reactive<Color>,
    dismiss: scrim::DismissFn,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let heading = Text::declaring(move || title.get(), LayoutStyle::new(), heading_style)?;

    let close_label = Text::declaring(
        || "Close".to_string(),
        LayoutStyle::new(),
        |t| shared::control_text(t, CLOSE_RATIO),
    )?;
    let close = StyledContainer::new(
        LayoutStyle::new().flex_row(),
        |_r| RectStyle::default(),
        vec![box_item(close_label)],
    )?
    .control(Role::Button)
    .on_press({
        let dismiss = dismiss.clone();
        move || (*dismiss)()
    });

    let header = Container::new(header_row(), vec![box_item(heading), box_item(close)])?
        .styled_by(header_row);

    let mut dialog_children: Vec<Box<dyn LayoutItem>> = vec![box_item(header)];
    dialog_children.extend(body);

    let dialog = StyledContainer::new(
        card(),
        move |_r| {
            RectStyle::default()
                .with_fill(shared::resolve(&color, shared::surface))
                .with_border(Border::uniform(shared::border(), 1.0))
                .with_radius(BorderRadius::all(dialog_radius()))
        },
        dialog_children,
    )?
    .styled_by(card)
    // Swallow taps on the card so only the scrim (or Close) dismisses.
    .on_press(|| {});

    let scrim = StyledContainer::new(
        backdrop(),
        |_r| RectStyle::default().with_fill(scrim::SCRIM),
        vec![box_item(dialog)],
    )?
    .styled_by(backdrop)
    .on_press(move || (*dismiss)());

    Ok(box_item(scrim))
}

#[cfg(test)]
#[path = "modal_test.rs"]
mod tests;
