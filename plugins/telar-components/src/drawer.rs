//! [`drawer`]: a panel that slides in from an edge over a dismissing scrim.

use std::rc::Rc;
#[cfg(test)]
use telar::Slots;
use telar::{
    Accessible, AlignItems, Border, BorderRadius, Children, Color, Container, Direction,
    JustifyContent, LayoutError, LayoutItem, LayoutScrollArea, LayoutStyle, Props, Reactive,
    RectStyle, RwSignal, ShapeStyle, StyledContainer, SurfaceAmendment, amend_surface, box_item,
    current_direction, focus::Role,
};

use crate::scrim;
use crate::shared;
use crate::strings;

/// A drawer's own tokens, which an application's theme supplies through a `#[theme(extension)]` field.
///
/// Every field left `None` follows the theme role or measure named beside it, read when the panel paints and lays out.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DrawerTokens {
    /// Radius of the two corners away from the edge the panel is pinned to. `None` is the theme's `radius_dialog`.
    pub radius: Option<f32>,
    /// Space between the panel's edge and its content, at the regular control size, scaled with the control size in force. `None` is [`metrics::gap_xl`](crate::metrics::gap_xl).
    pub pad: Option<f32>,
    /// The panel's width when the drawer names none, at the regular control size, scaled with the control size in force. `None` is 20 ems of the document's text.
    pub width: Option<f32>,
}

impl DrawerTokens {
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

    pub fn width(&self) -> f32 {
        shared::width_or_measure(self.width, 20.0)
    }
}

/// A side panel that covers the page from the left or right edge over a dimming scrim. When `open` is true it portals an `Overlay` with a translucent scrim and a full-height opaque panel pinned to `side`, holding the slot children; when false it collapses to a 0-size node. Tapping the scrim dismisses; tapping the panel does not. High-level sugar built on the `overlay` primitive; lives in `telar-components`, not the kernel.
///
/// The panel snaps in (no slide animation): the overlay is built only while open, so there is no off-screen frame to animate from. A slide-in (a `telar::motion::Animated<f32>` x-offset kept mounted across the close transition) is a follow-up.
#[derive(Props)]
pub struct DrawerProps {
    /// Bound open/close state. `None` (the default) never opens; `Some` drives the drawer.
    #[props(some, into, default)]
    pub open: Option<RwSignal<bool>>,
    /// Names this drawer, so anything on the same surface can open it with `open_overlay(id)` without holding its signal; another surface has its own `id`. `""` (the default) leaves it unnamed. Ignored when `open` is bound.
    #[props(default = "")]
    pub id: &'static str,
    /// Which edge the panel is pinned to: `"left"` (the default) or `"right"`.
    #[props(default = "left")]
    pub side: &'static str,
    /// Panel width in logical px. `0.0` (the default) means "unset" -> [`DrawerTokens::width`].
    #[props(default)]
    pub width: f32,
    /// Runs after the drawer sets `open = false`, so a caller can react to dismissal.
    #[props(some, default)]
    pub on_close: Option<Rc<dyn Fn()>>,
    /// Names the panel for a reader. `None` (the default) uses the localized `telar_components.drawer` message, which an application can override in its catalog.
    #[props(some, into, default)]
    pub label: Option<Reactive<String>>,
    /// Panel surface colour. `Color::TRANSPARENT` (the default) means "unset" -> the theme's `surface`. A closure (re-read every frame) so a theme token or `$signal` colour re-colours live.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub color: Reactive<Color>,
    /// Amends the paint of the panel — its **principal surface**. See [`telar::SurfaceStyle`] for when a theme token or [`DrawerTokens`] is the right instrument instead.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
}

/// A panel that slides in from an edge over a dismissing scrim.
pub fn drawer(props: DrawerProps, children: Children) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let DrawerProps {
        open,
        id,
        side,
        width,
        on_close,
        label,
        color,
        style,
    } = props;
    let open = shared::resolve_open(open, id);
    let width = (width > 0.0).then_some(width);
    // "right" pins the panel to the trailing edge; anything else (incl. the default "left") to the leading edge.
    let at_start = side != "right";
    let paint = Rc::new(move || amend_surface(panel_paint(&color, at_start), &style));

    scrim::scrim_overlay(open, on_close, move |dismiss| {
        // Built on the first open rather than here: a body built while the drawer has never opened would register its focus stops with nothing on screen to take them.
        let body = children.build()?.take_default();
        build_open_drawer(width, at_start, body, label, paint, dismiss)
    })
}

/// The panel's paint before the caller's amendment: rounded only on the side that faces the page.
fn panel_paint(color: &Reactive<Color>, at_start: bool) -> RectStyle {
    RectStyle::default()
        .with_fill(shared::resolve(color, shared::surface))
        .with_border(Border::uniform(shared::border(), shared::border_width()))
        .with_radius(inner_corners(at_start, DrawerTokens::current().radius()))
}

/// `radius` on the two corners away from the edge the panel is pinned to, which is the leading edge when `at_start` and is on the right in a right-to-left line.
fn inner_corners(at_start: bool, radius: f32) -> BorderRadius {
    let start_is_left = current_direction() != Direction::Rtl;
    let (left, right) = if at_start == start_is_left {
        (0.0, radius)
    } else {
        (radius, 0.0)
    };
    BorderRadius {
        top_left: left,
        top_right: right,
        bottom_right: right,
        bottom_left: left,
    }
}

/// Builds the scrim + full-height opaque panel for the open state: scrim (dims + dismisses) > panel pinned to `side`. The panel swallows its own taps so a click inside it never dismisses.
fn build_open_drawer(
    width: Option<f32>,
    at_start: bool,
    body: Vec<Box<dyn LayoutItem>>,
    label: Option<Reactive<String>>,
    paint: Rc<dyn Fn() -> RectStyle>,
    dismiss: scrim::DismissFn,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let room = scrim::Room::new();
    let sheet = move || {
        let tokens = DrawerTokens::current();
        LayoutStyle::new()
            .flex_column()
            .width(width.unwrap_or_else(|| tokens.width()))
            .max_width(room.inside(0.0).width)
            .min_height(0.0)
            .gap(shared::gap_lg())
            .padding_all(tokens.pad())
    };
    let name = Rc::new(move || match &label {
        Some(label) => label.get(),
        None => strings::text(strings::DRAWER),
    });
    let body = LayoutScrollArea::new(
        LayoutStyle::new().flex_shrink(1.0).min_height(0.0),
        box_item(Container::new(
            LayoutStyle::new().flex_column().gap(shared::gap_lg()),
            body,
        )?),
    )?
    .stop_label({
        let name = name.clone();
        move || name()
    });
    let panel = StyledContainer::new(sheet(), move |_r| paint(), vec![box_item(body)])?
        .styled_by(sheet)
        .role(Role::Dialog)
        .a11y_label(move || name())
        // Swallow taps on the panel so only the scrim dismisses.
        .on_press(|| {});

    // Cross axis (STRETCH) gives the panel full viewport height; the main axis (justify) pins it to the edge.
    let scrim = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .flex_grow(1.0)
            .align_items(AlignItems::STRETCH)
            .min_height(0.0)
            .justify_content(if at_start {
                JustifyContent::START
            } else {
                JustifyContent::END
            }),
        |_r| RectStyle::default().with_fill(shared::overlay()),
        vec![box_item(panel)],
    )?;
    room.follow(scrim.layout_node());
    let scrim = shared::dismissed_by_press(scrim, move || (*dismiss)());

    Ok(box_item(scrim))
}

#[cfg(test)]
#[path = "drawer_test.rs"]
mod tests;
