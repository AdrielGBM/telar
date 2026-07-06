use std::cell::{Cell, RefCell};
use std::rc::Rc;

use layout_core::{AlignItems, JustifyContent, LayoutError, LayoutStyle};
use reactive_core::RwSignal;
use renderer_core::{Color, RectStyle, ShapeStyle, Stroke};
use ui_core::{
    Container, LayoutItem, Overlay, ReactiveList, Slots, StyledContainer, WidgetCtx, box_item,
};

/// Scrim tone: a translucent black wash over the page (a fill, never an opacity layer, so the panel over it
/// stays opaque — see the note in `modal`).
const SCRIM: Color = Color::rgba(0.0, 0.0, 0.0, 0.5);
/// Fallback panel surface when `color` is unset — an opaque near-white sheet.
const DEFAULT_SURFACE: Color = Color::rgba(1.0, 1.0, 1.0, 1.0);
/// Hairline border on the panel's inner edge.
const DEFAULT_BORDER: Color = Color::rgba(0.12, 0.12, 0.16, 0.15);
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
    /// Panel surface colour. `Color::TRANSPARENT` (the default) means "unset" -> `DEFAULT_SURFACE`. A closure
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

pub fn drawer(
    ctx: &mut WidgetCtx,
    props: DrawerProps,
    mut slots: Slots,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
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

    let Some(open) = open else {
        return Ok(box_item(Container::new(
            ctx,
            LayoutStyle::new().width(0.0).height(0.0),
            vec![],
        )?));
    };

    // Built ONCE — lazily on the first open (host exists) — then kept mounted and shown/hidden via `open`, so
    // the pre-built slot body survives close/reopen (see `modal` for the latch + `Overlay::toggleable` rationale).
    let body = Rc::new(RefCell::new(Some(body)));
    let color: Rc<dyn Fn() -> Color> = Rc::from(color);
    let on_close: Option<Rc<dyn Fn()>> = on_close.map(Rc::from);

    let built = Rc::new(Cell::new(false));
    let key = {
        let open = open.clone();
        let built = built.clone();
        move || {
            if open.get() {
                built.set(true);
            }
            vec![built.get()]
        }
    };
    let list = ReactiveList::new(
        ctx,
        key,
        |b: &bool| *b,
        move |ctx, is_built| -> Result<Box<dyn LayoutItem>, LayoutError> {
            if !is_built {
                return Ok(box_item(Container::new(
                    ctx,
                    LayoutStyle::new().width(0.0).height(0.0),
                    vec![],
                )?));
            }
            build_open_drawer(
                ctx,
                width,
                justify,
                body.borrow_mut().take().unwrap_or_default(),
                color.clone(),
                open.clone(),
                on_close.clone(),
            )
        },
    )?;
    Ok(box_item(list))
}

/// Builds the portalled panel for the open state: `Overlay` > scrim (dims + dismisses) > full-height opaque
/// panel pinned to `side`. The panel swallows its own taps so a click inside it never dismisses.
fn build_open_drawer(
    ctx: &mut WidgetCtx,
    width: f32,
    justify: JustifyContent,
    body: Vec<Box<dyn LayoutItem>>,
    color: Rc<dyn Fn() -> Color>,
    open: RwSignal<bool>,
    on_close: Option<Rc<dyn Fn()>>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let panel = StyledContainer::new(
        ctx,
        LayoutStyle::new()
            .flex_column()
            .width(width)
            .gap(PANEL_GAP)
            .padding_all(PANEL_PAD),
        move |_r| {
            RectStyle::default()
                .with_fill(surface(color.as_ref()))
                .with_stroke(Stroke::new(DEFAULT_BORDER, 1.0))
        },
        body,
    )?
    // Swallow taps on the panel so only the scrim dismisses.
    .on_press(|| {});

    // Cross axis (STRETCH) gives the panel full viewport height; the main axis (justify) pins it to the edge.
    let scrim = StyledContainer::new(
        ctx,
        LayoutStyle::new()
            .flex_row()
            .flex_grow(1.0)
            .align_items(AlignItems::STRETCH)
            .justify_content(justify),
        |_r| RectStyle::default().with_fill(SCRIM),
        vec![box_item(panel)],
    )?
    .on_press(dismiss(open.clone(), on_close));

    // Kept mounted; shown only while `open`, so the pre-built body survives a close/reopen.
    let overlay = Overlay::toggleable(
        ctx,
        LayoutStyle::new().flex_column(),
        vec![box_item(scrim)],
        move || open.get(),
    )?;
    Ok(box_item(overlay))
}

/// A dismiss handler: set `open = false`, then run `on_close`.
fn dismiss(open: RwSignal<bool>, on_close: Option<Rc<dyn Fn()>>) -> impl Fn() + 'static {
    move || {
        open.set(false);
        if let Some(cb) = &on_close {
            cb();
        }
    }
}

/// The panel surface: the caller's reactive `color` if set, else the opaque default sheet colour.
fn surface(color: &dyn Fn() -> Color) -> Color {
    let c = color();
    if c == Color::TRANSPARENT {
        DEFAULT_SURFACE
    } else {
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use layout_core::AvailableSpace;
    use reactive_core::signal;
    use renderer_core::{DrawCommand, TextStyle};
    use ui_core::{ComponentList, Text, compute_layout, new_container, relayout_if_dirty};

    fn find_text(cmds: &[DrawCommand], needle: &str) -> bool {
        cmds.iter()
            .any(|c| matches!(c, DrawCommand::Text { text, .. } if text.as_ref() == needle))
    }

    fn slot_with_body(ctx: &mut WidgetCtx, label: &'static str) -> Slots {
        let body = Text::new(
            ctx,
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
        let mut ctx = WidgetCtx::new();
        let open = signal(false);
        let slots = slot_with_body(&mut ctx, "Drawer body");
        let drawer = drawer(
            &mut ctx,
            DrawerProps {
                open: Some(open.clone()),
                side: "right",
                ..Default::default()
            },
            slots,
        )
        .unwrap();

        let root = new_container(
            &mut ctx,
            LayoutStyle::new().flex_column().width(400.0).height(400.0),
            &[drawer.layout_node()],
        )
        .unwrap();
        compute_layout(
            &mut ctx,
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
        let mut ctx = WidgetCtx::new();
        let slots = slot_with_body(&mut ctx, "Drawer body");
        let drawer = drawer(&mut ctx, DrawerProps::default(), slots).unwrap();
        let tree = ComponentList::new(drawer);
        assert!(!find_text(&tree.commands(), "Drawer body"));
    }
}
