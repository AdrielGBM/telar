//! [`drawer`]: a panel that slides in from an edge over a dismissing scrim.

use std::rc::Rc;
use telar::{
    Accessible, AlignItems, Border, BorderRadius, Children, Color, Container, Direction,
    JustifyContent, LayoutError, LayoutItem, LayoutScrollArea, LayoutStyle, OverlayControl, Props,
    Reactive, RectStyle, ShapeStyle, Side, StyledContainer, SurfaceAmendment, amend_surface,
    box_item, children, current_direction, focus::Role,
};

use crate::shared;
use crate::shared::scrim;
use crate::strings;
use telar::ControlSize;

/// A drawer's own tokens, which an application's theme supplies through a `#[theme(extension)]` field.
///
/// Every field left `None` follows the theme role or measure named beside it, read when the panel paints and lays out.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DrawerTokens {
    /// Radius of the two corners away from the edge the panel is pinned to. `None` is the theme's `radius_dialog`.
    pub radius: Option<f32>,
    /// Space between the panel's edge and its content, at the regular size, scaled with the drawer's own. `None` is [`metrics::gap_xl`](crate::metrics::gap_xl).
    pub pad: Option<f32>,
    /// How far the panel reaches in from its edge when the drawer names no extent, at the regular size, scaled with the drawer's own. `None` is 20 ems of the document's text.
    pub extent: Option<f32>,
}

impl DrawerTokens {
    /// The tokens of the theme in force, subscribing the caller to it.
    pub fn current() -> Self {
        telar::use_theme_extension::<Self>()
    }

    pub fn radius(&self) -> f32 {
        self.radius.unwrap_or_else(shared::radius_dialog)
    }

    /// The padding of a drawer of `size`.
    pub fn pad(&self, size: ControlSize) -> f32 {
        self.pad
            .map_or_else(|| shared::gap_xl(size), |pad| pad * size.scale())
    }

    /// How far the panel reaches in from its edge in a drawer of `size` whose call names no extent.
    pub fn extent(&self, size: ControlSize) -> f32 {
        shared::width_or_measure(size, self.extent, 20.0)
    }
}

/// A panel that covers the page from one of its edges over a dimming scrim. While its `control` is open it portals an `Overlay` with a translucent scrim and an opaque panel pinned to `side` and spanning that edge, holding its `children`; when false it collapses to a 0-size node. Tapping the scrim dismisses; tapping the panel does not. High-level sugar built on the `overlay` primitive; lives in `telar-components`, not the kernel.
///
/// The panel snaps in (no slide animation): the overlay is built only while open, so there is no off-screen frame to animate from. A slide-in (a `telar::motion::Animated<f32>` x-offset kept mounted across the close transition) is a follow-up.
#[derive(Props)]
#[props(layer)]
pub struct DrawerProps {
    /// What opens and closes the drawer: an [`OverlayName`](telar::OverlayName) anything on the same surface opens with [`open_overlay`](telar::open_overlay), each surface keeping its own, or a signal the caller holds.
    #[props(into)]
    pub control: OverlayControl,
    /// Which edge the panel is pinned to, [`Side::Start`] by default: the start and the end of a line follow the way the surface reads, so a drawer at the end opens on the right of a left-to-right surface and on the left of a right-to-left one.
    #[props(default = Side::Start)]
    pub side: Side,
    /// How far the panel reaches in from its edge, in logical px: its width at the start or the end, its height above or below. `None` (the default) is [`DrawerTokens::extent`].
    #[props(some, default)]
    pub extent: Option<f32>,
    /// How large the panel is drawn: its padding, gaps and, unless it names one, its extent, as [`ControlSize`] works them out. What it holds is sized where it is written.
    #[props(default)]
    pub size: ControlSize,
    /// Runs after the drawer sets `open = false`, so a caller can react to dismissal.
    #[props(some, default)]
    pub on_close: Option<Rc<dyn Fn()>>,
    /// Names the panel for a reader. `None` (the default) uses the localized `telar_components.drawer` message, which an application can override in its catalog.
    #[props(some, into, default)]
    pub label: Option<Reactive<String>>,
    /// Panel surface colour. `None` (the default) is the theme's `surface`. A closure (re-read every frame) so a theme token or `$signal` colour re-colours live.
    #[props(some, into, default)]
    pub color: Option<Reactive<Color>>,
    /// Amends the paint of the panel — its **principal surface**. See [`telar::SurfaceStyle`] for when a theme token or [`DrawerTokens`] is the right instrument instead.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
    /// What the panel holds.
    #[props(default)]
    pub children: Children,
}

/// A panel that slides in from an edge over a dismissing scrim.
pub fn drawer(props: DrawerProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let DrawerProps {
        control,
        side,
        extent,
        size,
        on_close,
        label,
        color,
        style,
        children,
    } = props;
    let open = control.state();
    let paint = Rc::new(move || amend_surface(panel_paint(&color, side), &style));

    scrim::scrim_overlay(open, on_close, move |dismiss| {
        // Built on the first open rather than here: a body built while the drawer has never opened would register its focus stops with nothing on screen to take them.
        let body = children.build()?;
        build_open_drawer(
            size,
            extent,
            side,
            body,
            label.clone(),
            paint.clone(),
            dismiss,
        )
    })
}

/// The panel's paint before the caller's amendment: rounded only on the corners that face the page.
fn panel_paint(color: &Option<Reactive<Color>>, side: Side) -> RectStyle {
    RectStyle::default()
        .with_fill(shared::resolve(color, shared::surface))
        .with_border(Border::uniform(shared::border(), shared::border_width()))
        .with_radius(inner_corners(side, DrawerTokens::current().radius()))
}

/// `radius` on the two corners away from the edge the panel is pinned to, that edge resolved against the way the surface reads.
fn inner_corners(side: Side, radius: f32) -> BorderRadius {
    let start_is_left = current_direction() != Direction::Rtl;
    let left = match side {
        Side::Start => start_is_left,
        Side::End => !start_is_left,
        Side::Above | Side::Below => false,
    };
    let right = matches!(side, Side::Start | Side::End) && !left;
    let (top, bottom) = (side == Side::Above, side == Side::Below);
    let corner = |on_pinned_edge: bool| match on_pinned_edge {
        true => 0.0,
        false => radius,
    };
    BorderRadius {
        top_left: corner(top || left),
        top_right: corner(top || right),
        bottom_right: corner(bottom || right),
        bottom_left: corner(bottom || left),
    }
}

/// Builds the scrim and the opaque panel for the open state: the scrim dims and dismisses, the panel is pinned to `side` and spans that edge. The panel swallows its own taps so a click inside it never dismisses.
fn build_open_drawer(
    size: ControlSize,
    extent: Option<f32>,
    side: Side,
    body: Vec<Box<dyn LayoutItem>>,
    label: Option<Reactive<String>>,
    paint: Rc<dyn Fn() -> RectStyle>,
    dismiss: scrim::DismissFn,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let room = scrim::Room::new();
    let across = matches!(side, Side::Start | Side::End);
    let sheet = move || {
        let tokens = DrawerTokens::current();
        let reach = extent.unwrap_or_else(|| tokens.extent(size));
        let room = room.inside(0.0);
        let sheet = match across {
            true => LayoutStyle::column()
                .width(reach)
                .max_width(room.width)
                .min_height(0.0),
            false => LayoutStyle::column()
                .height(reach)
                .max_height(room.height)
                .min_width(0.0),
        };
        sheet
            .gap(shared::gap_lg(size))
            .padding_all(tokens.pad(size))
    };
    let name = Rc::new(move || match &label {
        Some(label) => label.get(),
        None => strings::text(strings::DRAWER),
    });
    let body = LayoutScrollArea::new(
        LayoutStyle::new().flex_shrink(1.0),
        box_item(Container::new(
            LayoutStyle::column().gap(shared::gap_lg(size)),
            body,
        )?),
    )?
    .stop_label({
        let name = name.clone();
        move || name()
    });
    let panel = StyledContainer::new(sheet(), move |_r| paint(), children![body])?
        .styled_by(sheet)
        .role(Role::Dialog)
        .a11y_label(move || name())
        // Swallow taps on the panel so only the scrim dismisses.
        .on_press(|| {});

    // The cross axis (STRETCH) spans the panel along its edge; the main axis (justify) pins it there, a row's start and end following the way the surface reads.
    let flow = match across {
        true => LayoutStyle::row().min_height(0.0),
        false => LayoutStyle::column().min_width(0.0),
    };
    let pinned = match side {
        Side::Start | Side::Above => JustifyContent::START,
        Side::End | Side::Below => JustifyContent::END,
    };
    let scrim = StyledContainer::new(
        flow.flex_grow(1.0)
            .align_items(AlignItems::STRETCH)
            .justify_content(pinned),
        |_r| RectStyle::default().with_fill(shared::overlay()),
        children![panel],
    )?;
    room.follow(scrim.layout_node());
    let scrim = shared::dismissed_by_press(scrim, move || (*dismiss)());

    Ok(box_item(scrim))
}

#[cfg(test)]
#[path = "drawer.test.rs"]
mod tests;
