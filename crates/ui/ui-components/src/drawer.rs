use layout_core::{AlignItems, JustifyContent, LayoutError, LayoutStyle};
use reactive_core::RwSignal;
use renderer_core::{Color, RectStyle, ShapeStyle, Stroke};
use ui_core::{LayoutItem, Slots, StyledContainer, box_item};

use crate::scrim;
use crate::shared;

/// Panel width when `width` is unset (`0.0`).
const DEFAULT_WIDTH: f32 = 280.0;
const PANEL_PAD: f32 = 20.0;
const PANEL_GAP: f32 = 12.0;

/// A side panel that covers the page from the left or right edge over a dimming scrim. When `open` is true it
/// portals an `Overlay` with a translucent scrim and a full-height opaque panel pinned to `side`, holding the
/// slot children; when false it collapses to a 0-size node. Tapping the scrim dismisses; tapping the panel
/// does not. High-level sugar built on the `overlay` primitive; lives in `ui-components`, not the kernel.
///
/// The panel snaps in (no slide animation): the overlay is built only while open, so there is no off-screen
/// frame to animate from. A slide-in (a `motion_core::Animated<f32>` x-offset kept mounted across the close
/// transition) is a follow-up.
pub struct DrawerProps {
    /// Bound open/close state. `None` (the default) never opens; `Some` drives the drawer.
    pub open: Option<RwSignal<bool>>,
    /// Which edge the panel is pinned to: `"left"` (the default) or `"right"`.
    pub side: &'static str,
    /// Panel width in logical px. `0.0` (the default) means "unset" -> `DEFAULT_WIDTH`.
    pub width: f32,
    /// Runs after the drawer sets `open = false`, so a caller can react to dismissal.
    pub on_close: Option<Box<dyn Fn()>>,
    /// Panel surface colour. `Color::TRANSPARENT` (the default) means "unset" -> `shared::DEFAULT_SURFACE`. A closure
    /// (re-read every frame) so a theme token or `$signal` colour re-colours live.
    pub color: Box<dyn Fn() -> Color>,
}

impl Default for DrawerProps {
    fn default() -> Self {
        Self {
            open: None,
            side: "left",
            width: 0.0,
            on_close: None,
            color: Box::new(|| Color::TRANSPARENT),
        }
    }
}

pub fn drawer(props: DrawerProps, mut slots: Slots) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let DrawerProps {
        open,
        side,
        width,
        on_close,
        color,
    } = props;
    let body = slots.take_default();
    let width = if width > 0.0 { width } else { DEFAULT_WIDTH };
    // "right" pins the panel to the trailing edge; anything else (incl. the default "left") to the leading edge.
    let justify = if side == "right" {
        JustifyContent::END
    } else {
        JustifyContent::START
    };

    scrim::scrim_overlay(open, on_close, move |dismiss| {
        build_open_drawer(width, justify, body, color, dismiss)
    })
}

/// Builds the scrim + full-height opaque panel for the open state: scrim (dims + dismisses) > panel pinned to
/// `side`. The panel swallows its own taps so a click inside it never dismisses.
fn build_open_drawer(
    width: f32,
    justify: JustifyContent,
    body: Vec<Box<dyn LayoutItem>>,
    color: Box<dyn Fn() -> Color>,
    dismiss: scrim::DismissFn,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let panel = StyledContainer::new(
        LayoutStyle::new()
            .flex_column()
            .width(width)
            .gap(PANEL_GAP)
            .padding_all(PANEL_PAD),
        move |_r| {
            RectStyle::default()
                .with_fill(shared::resolve(color.as_ref(), || shared::DEFAULT_SURFACE))
                .with_stroke(Stroke::new(scrim::DEFAULT_BORDER, 1.0))
        },
        body,
    )?
    // Swallow taps on the panel so only the scrim dismisses.
    .on_press(|| {});

    // Cross axis (STRETCH) gives the panel full viewport height; the main axis (justify) pins it to the edge.
    let scrim = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .flex_grow(1.0)
            .align_items(AlignItems::STRETCH)
            .justify_content(justify),
        |_r| RectStyle::default().with_fill(scrim::SCRIM),
        vec![box_item(panel)],
    )?
    .on_press(move || (*dismiss)());

    Ok(box_item(scrim))
}

#[cfg(test)]
mod tests {
    use super::*;
    use layout_core::AvailableSpace;
    use reactive_core::signal;
    use renderer_core::{DrawCommand, TextStyle};
    use ui_core::reset_layout_runtime;
    use ui_core::{ComponentList, Text, compute_layout, new_container, relayout_if_dirty};

    fn find_text(cmds: &[DrawCommand], needle: &str) -> bool {
        cmds.iter()
            .any(|c| matches!(c, DrawCommand::Text { text, .. } if text.as_ref() == needle))
    }

    fn slot_with_body(label: &'static str) -> Slots {
        let body = Text::new(
            move || label.to_string(),
            LayoutStyle::new().height(20.0),
            || TextStyle::new(14.0, Color::BLACK),
        )
        .unwrap();
        let mut slots = Slots::new();
        slots.push(None, box_item(body));
        slots
    }

    // Toggling `open` shows then hides the panel: the body is composed only while open, and the Overlay
    // portal is disposed when it closes (its content leaves the command stream).
    #[test]
    fn open_shows_panel_and_close_hides_it() {
        reset_layout_runtime();
        let open = signal(false);
        let slots = slot_with_body("Drawer body");
        let drawer = drawer(
            DrawerProps {
                open: Some(open.clone()),
                side: "right",
                ..Default::default()
            },
            slots,
        )
        .unwrap();

        let root = new_container(
            LayoutStyle::new().flex_column().width(400.0).height(400.0),
            &[drawer.layout_node()],
        )
        .unwrap();
        compute_layout(
            root,
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(400.0),
        )
        .unwrap();
        let tree = ComponentList::new(drawer);

        assert!(!find_text(&tree.commands(), "Drawer body"));

        open.set(true);
        relayout_if_dirty();
        assert!(
            find_text(&tree.commands(), "Drawer body"),
            "body shows when open"
        );

        open.set(false);
        relayout_if_dirty();
        assert!(
            !find_text(&tree.commands(), "Drawer body"),
            "body hidden when closed"
        );
    }

    // An unbound drawer (no `open` signal) builds a 0-size node and never portals anything.
    #[test]
    fn unbound_drawer_renders_nothing() {
        reset_layout_runtime();
        let slots = slot_with_body("Drawer body");
        let drawer = drawer(DrawerProps::default(), slots).unwrap();
        let tree = ComponentList::new(drawer);
        assert!(!find_text(&tree.commands(), "Drawer body"));
    }
}
