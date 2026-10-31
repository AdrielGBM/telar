//! [`modal`]: a dialog portaled over the page, dismissed by its scrim, its Close or Escape.

use std::rc::Rc;
use telar::{
    Accessible, AlignItems, Border, BorderRadius, Children, Color, Container, JustifyContent,
    LayoutError, LayoutItem, LayoutScrollArea, LayoutStyle, Props, Reactive, RectStyle, RwSignal,
    ShapeStyle, StyledContainer, SurfaceAmendment, Text, amend_surface, box_item, children,
    focus::Role,
};

use crate::heading::{HeadingProps, heading};
use crate::scrim;
use crate::shared;
use crate::strings;

/// A modal's own tokens, which an application's theme supplies through a `#[theme(extension)]` field.
///
/// Every field left `None` follows the theme role or measure named beside it, read when the dialog paints and lays out.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModalTokens {
    /// Corner radius. `None` is the theme's `radius_dialog`.
    pub radius: Option<f32>,
    /// Space between the dialog's edge and its content, and between the dialog and the window's edge, at the regular control size, scaled with the control size in force. `None` is [`metrics::gap_xl`](crate::metrics::gap_xl).
    pub pad: Option<f32>,
    /// The narrowest the dialog is drawn, at the regular control size, scaled with the control size in force. `None` is 24 ems of the document's text.
    pub min_width: Option<f32>,
    /// The widest the dialog is drawn, at the regular control size, scaled with the control size in force. `None` is 32 ems of the document's text.
    pub max_width: Option<f32>,
}

impl ModalTokens {
    /// The tokens of the theme in force, subscribing the caller to it.
    pub fn current() -> Self {
        telar::use_theme_extension::<Self>()
    }

    pub fn radius(&self) -> f32 {
        self.radius.unwrap_or_else(shared::radius_dialog)
    }

    pub fn pad(&self) -> f32 {
        self.pad
            .map_or_else(shared::gap_xl, |pad| pad * telar::control_scale())
    }

    pub fn min_width(&self) -> f32 {
        shared::width_or_measure(self.min_width, 24.0)
    }

    pub fn max_width(&self) -> f32 {
        shared::width_or_measure(self.max_width, 32.0)
    }
}

/// The close glyph's share of the text around it.
const CLOSE_RATIO: f32 = 0.93;

fn header_row() -> LayoutStyle {
    LayoutStyle::row()
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::SPACE_BETWEEN)
        .gap(shared::gap_lg())
}
fn card(room: scrim::Room) -> LayoutStyle {
    let tokens = ModalTokens::current();
    let room = room.inside(tokens.pad());
    LayoutStyle::column()
        .gap(shared::gap_lg())
        // So the dialog never collapses to its min-content width: the header and body leaves are unmeasured, with 0 intrinsic width, which would shrink the card to a strip. Capped by the room, so a narrow window wins over the minimum.
        .min_width(tokens.min_width().min(room.width))
        .max_width(tokens.max_width().min(room.width))
        .max_height(room.height)
        .padding_all(tokens.pad())
}
fn body_column() -> LayoutStyle {
    LayoutStyle::column().gap(shared::gap_lg())
}
fn body_scroll() -> LayoutStyle {
    LayoutStyle::new().flex_shrink(1.0)
}
fn backdrop() -> LayoutStyle {
    LayoutStyle::column()
        .flex_grow(1.0)
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER)
        .min_height(0.0)
        .padding_all(ModalTokens::current().pad())
}

/// A centred dialog over a dimming scrim. When `open` is true it portals an `Overlay` (a top-layer layer that escapes clipping and blocks clicks behind it) with a translucent scrim and, centred on it, an opaque surface card holding the `title`, its `children` (the dialog body) and a Close affordance; when false it collapses to a 0-size node. Tapping the scrim (or Close) dismisses; tapping the card itself does not. High-level sugar built on the `overlay` primitive; lives in `telar-components`, not the kernel.
#[derive(Props)]
pub struct ModalProps {
    /// Bound open/close state. `None` (the default) never opens (no signal to read); `Some` drives the modal.
    #[props(some, into, default)]
    pub open: Option<RwSignal<bool>>,
    /// Names this dialog, so anything on the same surface can open it with `open_overlay(id)` without holding its signal; another surface has its own `id`. `""` (the default) leaves it unnamed. Ignored when `open` is bound — an explicit signal wins, so the two forms never disagree about which state is authoritative.
    #[props(default = "")]
    pub id: &'static str,
    /// The dialog's heading, shown beside its Close affordance.
    #[props(into, default)]
    pub title: Reactive<String>,
    /// Runs after the modal sets `open = false` (scrim tap or Close), so a caller can react to dismissal.
    #[props(some, default)]
    pub on_close: Option<Rc<dyn Fn()>>,
    /// The Close affordance's text. `None` (the default) uses the localized `telar_components.close` message, which an application can override in its catalog; `Some` wins over both.
    #[props(some, into, default)]
    pub close_label: Option<Reactive<String>>,
    /// Dialog surface colour. `Color::TRANSPARENT` (the default) means "unset" -> the theme's `surface`. A closure (re-read every frame) so a theme token or `$signal` colour re-colours live, like `button`'s `fill`.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub color: Reactive<Color>,
    /// Amends the paint of the dialog's card — its **principal surface**. See [`telar::SurfaceStyle`] for when a theme token or [`ModalTokens`] is the right instrument instead.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
    /// The dialog body, under its title.
    #[props(default)]
    pub children: Children,
}

/// A dialog portaled over the page, dismissed by its scrim, its Close or Escape.
pub fn modal(props: ModalProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let ModalProps {
        open,
        id,
        title,
        on_close,
        close_label,
        color,
        style,
        children,
    } = props;
    let open = shared::resolve_open(open, id);
    let paint: Rc<dyn Fn() -> RectStyle> =
        Rc::new(move || amend_surface(card_paint(&color), &style));

    scrim::scrim_overlay(open, on_close, move |dismiss| {
        // Built on the first open rather than here: a body built while the dialog has never opened would register its focus stops with nothing on screen to take them.
        let body = children.build()?;
        build_open_modal(
            title.clone(),
            close_label.clone(),
            body,
            paint.clone(),
            dismiss,
        )
    })
}

/// Builds the scrim + centred opaque card for the open state: scrim (dims + dismisses) > centred card (title row with Close, then the body). The card swallows its own taps so a click inside it never dismisses.
fn build_open_modal(
    title: Reactive<String>,
    close_label: Option<Reactive<String>>,
    body: Vec<Box<dyn LayoutItem>>,
    paint: Rc<dyn Fn() -> RectStyle>,
    dismiss: scrim::DismissFn,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let name = title.clone();
    let body_name = title.clone();
    let heading = heading(HeadingProps::props().text(title).build())?;

    let close_label = Text::declaring(
        move || match &close_label {
            Some(label) => label.get(),
            None => strings::text(strings::CLOSE),
        },
        LayoutStyle::new(),
        |t| shared::control_text(t, CLOSE_RATIO),
    )?;
    let close = StyledContainer::new(
        LayoutStyle::row(),
        |_r| RectStyle::default(),
        children![close_label],
    )?
    .control(Role::Button)
    .on_press({
        let dismiss = dismiss.clone();
        move || (*dismiss)()
    });

    let header = Container::new(header_row(), children![heading, close])?.styled_by(header_row);

    let body = LayoutScrollArea::new(
        body_scroll(),
        box_item(Container::new(body_column(), body)?),
    )?
    .stop_label(move || body_name.get());
    let dialog_children: Vec<Box<dyn LayoutItem>> = children![header, body];

    let room = scrim::Room::new();
    let dialog = StyledContainer::new(card(room), move |_r| paint(), dialog_children)?
        .styled_by(move || card(room))
        .role(Role::Dialog)
        .a11y_label(move || name.get())
        // Swallow taps on the card so only the scrim (or Close) dismisses.
        .on_press(|| {});

    let scrim = StyledContainer::new(
        backdrop(),
        |_r| RectStyle::default().with_fill(shared::overlay()),
        children![dialog],
    )?
    .styled_by(backdrop);
    room.follow(scrim.layout_node());
    let scrim = shared::dismissed_by_press(scrim, move || (*dismiss)());

    Ok(box_item(scrim))
}

/// The card's paint before the caller's amendment.
fn card_paint(color: &Reactive<Color>) -> RectStyle {
    RectStyle::default()
        .with_fill(shared::resolve(color, shared::surface))
        .with_border(Border::uniform(shared::border(), shared::border_width()))
        .with_radius(BorderRadius::all(ModalTokens::current().radius()))
}

#[cfg(test)]
#[path = "modal_test.rs"]
mod tests;
