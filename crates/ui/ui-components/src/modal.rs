use std::cell::{Cell, RefCell};
use std::rc::Rc;

use layout_core::{AlignItems, JustifyContent, LayoutError, LayoutStyle};
use reactive_core::RwSignal;
use renderer_core::{BorderRadius, Color, RectStyle, ShapeStyle, Stroke, TextStyle};
use ui_core::{
    Container, LayoutItem, Overlay, ReactiveList, Slots, StyledContainer, Text, WidgetCtx, box_item,
};

use crate::heading::heading_style;

/// Scrim tone: a translucent black wash over the page. Kept as a *fill* on an opaque-subtree dialog (never
/// an `opacity` layer over the whole overlay) so the dialog itself stays fully opaque — layering the scrim's
/// alpha over the dialog would bleed the page through it.
const SCRIM: Color = Color::rgba(0.0, 0.0, 0.0, 0.5);
/// Fallback dialog surface when `color` is unset — an opaque near-white card.
const DEFAULT_SURFACE: Color = Color::rgba(1.0, 1.0, 1.0, 1.0);
/// Hairline border around the dialog card.
const DEFAULT_BORDER: Color = Color::rgba(0.12, 0.12, 0.16, 0.15);
/// Muted tone for the Close affordance.
const CLOSE_INK: Color = Color::rgba(0.35, 0.35, 0.42, 1.0);
const DIALOG_RADIUS: f32 = 12.0;
const DIALOG_PAD: f32 = 24.0;
const DIALOG_GAP: f32 = 12.0;
const CLOSE_SIZE: f32 = 13.0;

/// A centred dialog over a dimming scrim. When `open` is true it portals an `Overlay` (a top-layer layer that
/// escapes clipping and blocks clicks behind it) with a translucent scrim and, centred on it, an opaque
/// surface card holding the `title`, the slot children (the dialog body) and a Close affordance; when false
/// it collapses to a 0-size node. Tapping the scrim (or Close) dismisses; tapping the card itself does not.
/// High-level sugar built on the `overlay` primitive; lives in `ui-components`, not the kernel.
pub struct ModalProps {
    /// Bound open/close state. `None` (the default) never opens (no signal to read); `Some` drives the modal.
    pub open: Option<RwSignal<bool>>,
    pub title: &'static str,
    /// Runs after the modal sets `open = false` (scrim tap or Close), so a caller can react to dismissal.
    pub on_close: Option<Box<dyn Fn()>>,
    /// Dialog surface colour. `Color::TRANSPARENT` (the default) means "unset" -> `DEFAULT_SURFACE`. A closure
    /// (re-read every frame) so a theme token or `$signal` colour re-colours live, like `button`'s `fill`.
    pub color: Box<dyn Fn() -> Color>,
}

impl Default for ModalProps {
    fn default() -> Self {
        Self {
            open: None,
            title: "",
            on_close: None,
            color: Box::new(|| Color::TRANSPARENT),
        }
    }
}

pub fn modal(
    ctx: &mut WidgetCtx,
    props: ModalProps,
    mut slots: Slots,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let ModalProps {
        open,
        title,
        on_close,
        color,
    } = props;
    let body = slots.take_default();

    // Unbound: the modal can never open, so render nothing and drop the body.
    let Some(open) = open else {
        return Ok(box_item(Container::new(
            ctx,
            LayoutStyle::new().width(0.0).height(0.0),
            vec![],
        )?));
    };

    // The slot body arrives pre-built (a `Box<dyn LayoutItem>` can't be rebuilt once consumed). So the dialog
    // is built ONCE — lazily on the first open, so the layout host exists and the portal attaches to the
    // viewport — then kept mounted and shown/hidden via `open` (see `Overlay::toggleable`), preserving the body
    // across close/reopen. The `ReactiveList` is keyed on a latch that flips true on the first open and stays
    // true, so the built dialog is never disposed. Colour/close callbacks are `Rc` for the build closure.
    let body = Rc::new(RefCell::new(Some(body)));
    let color: Rc<dyn Fn() -> Color> = Rc::from(color);
    let on_close: Option<Rc<dyn Fn()>> = on_close.map(Rc::from);

    let built = Rc::new(Cell::new(false));
    let key = {
        let open = open.clone();
        let built = built.clone();
        move || {
            // Latch on first open; reading `open` subscribes the list so it re-runs to build the dialog then.
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
                // Never opened: an empty placeholder (no overlay registered, nothing blocks the page).
                return Ok(box_item(Container::new(
                    ctx,
                    LayoutStyle::new().width(0.0).height(0.0),
                    vec![],
                )?));
            }
            build_open_modal(
                ctx,
                title,
                body.borrow_mut().take().unwrap_or_default(),
                color.clone(),
                open.clone(),
                on_close.clone(),
            )
        },
    )?;
    Ok(box_item(list))
}

/// Builds the portalled dialog for the open state: `Overlay` > scrim (dims + dismisses) > centred opaque card
/// (title row with Close, then the body). The card swallows its own taps so a click inside it never dismisses.
fn build_open_modal(
    ctx: &mut WidgetCtx,
    title: &'static str,
    body: Vec<Box<dyn LayoutItem>>,
    color: Rc<dyn Fn() -> Color>,
    open: RwSignal<bool>,
    on_close: Option<Rc<dyn Fn()>>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    // `auto` (measured) so the title/Close get their intrinsic WIDTH in the header row; a plain `Text::new`
    // only stretches its cross-axis (height in a row), leaving width 0 and the text invisible.
    let heading = Text::auto(
        ctx,
        move || title.to_string(),
        LayoutStyle::new(),
        heading_style,
    )?;

    let close_label = Text::auto(
        ctx,
        || "Close".to_string(),
        LayoutStyle::new(),
        || TextStyle::new(CLOSE_SIZE, CLOSE_INK),
    )?;
    let close = StyledContainer::new(
        ctx,
        LayoutStyle::new().flex_row(),
        |_r| RectStyle::default(),
        vec![box_item(close_label)],
    )?
    .on_press(dismiss(open.clone(), on_close.clone()));

    let header = Container::new(
        ctx,
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::SPACE_BETWEEN)
            .gap(DIALOG_GAP),
        vec![box_item(heading), box_item(close)],
    )?;

    let mut dialog_children: Vec<Box<dyn LayoutItem>> = vec![box_item(header)];
    dialog_children.extend(body);

    let dialog = StyledContainer::new(
        ctx,
        LayoutStyle::new()
            .flex_column()
            .gap(DIALOG_GAP)
            // A min width so the dialog never collapses to its content's min-content width (the header/body
            // `Text` leaves are unmeasured → 0 intrinsic width, which otherwise shrinks the card to a strip).
            .min_width(320.0)
            .max_width(440.0)
            .padding_all(DIALOG_PAD),
        move |_r| {
            RectStyle::default()
                .with_fill(surface(color.as_ref()))
                .with_stroke(Stroke::new(DEFAULT_BORDER, 1.0))
                .with_radius(BorderRadius::all(DIALOG_RADIUS))
        },
        dialog_children,
    )?
    // Swallow taps on the card so only the scrim (or Close) dismisses.
    .on_press(|| {});

    let scrim = StyledContainer::new(
        ctx,
        LayoutStyle::new()
            .flex_column()
            .flex_grow(1.0)
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER)
            .padding_all(DIALOG_PAD),
        |_r| RectStyle::default().with_fill(SCRIM),
        vec![box_item(dialog)],
    )?
    .on_press(dismiss(open.clone(), on_close));

    // Kept mounted; shown only while `open`. So the pre-built body survives a close/reopen (it is not rebuilt).
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

/// The dialog surface: the caller's reactive `color` if set, else the opaque default card colour.
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
    use renderer_core::DrawCommand;
    use ui_core::{ComponentList, compute_layout, new_container, relayout_if_dirty};

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

    // Toggling `open` shows then hides the dialog: the title and body are composed only while open, and the
    // Overlay portal is disposed when it closes (its content leaves the command stream).
    #[test]
    fn open_shows_dialog_and_close_hides_it() {
        let mut ctx = WidgetCtx::new();
        let open = signal(false);
        let slots = slot_with_body(&mut ctx, "Body");
        let modal = modal(
            &mut ctx,
            ModalProps {
                open: Some(open.clone()),
                title: "Confirm",
                ..Default::default()
            },
            slots,
        )
        .unwrap();

        // A parent-less root computed against the window registers the overlay host the portal attaches to.
        let root = new_container(
            &mut ctx,
            LayoutStyle::new().flex_column().width(400.0).height(400.0),
            &[modal.layout_node()],
        )
        .unwrap();
        compute_layout(
            &mut ctx,
            root,
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(400.0),
        )
        .unwrap();
        let tree = ComponentList::new(modal);

        // Closed: neither the title nor the body is drawn.
        assert!(!find_text(&tree.commands(), "Confirm"));
        assert!(!find_text(&tree.commands(), "Body"));

        // Open: the portal mounts, is laid out into the host, and its dialog composes on top.
        open.set(true);
        relayout_if_dirty();
        assert!(
            find_text(&tree.commands(), "Confirm"),
            "title shows when open"
        );
        assert!(find_text(&tree.commands(), "Body"), "body shows when open");

        // Close: the dialog is hidden (kept mounted, draws nothing) and its content leaves the command stream.
        open.set(false);
        relayout_if_dirty();
        assert!(
            !find_text(&tree.commands(), "Confirm"),
            "title hidden when closed"
        );
        assert!(
            !find_text(&tree.commands(), "Body"),
            "body hidden when closed"
        );

        // Reopen: the SAME pre-built body must reappear (the take-once bug lost it on the second open).
        open.set(true);
        relayout_if_dirty();
        assert!(
            find_text(&tree.commands(), "Confirm"),
            "title shows again on reopen"
        );
        assert!(
            find_text(&tree.commands(), "Body"),
            "body must survive a close/reopen (kept mounted, not rebuilt from a consumed slot)"
        );
    }

    // An unbound modal (no `open` signal) builds a 0-size node and never portals anything.
    #[test]
    fn unbound_modal_renders_nothing() {
        let mut ctx = WidgetCtx::new();
        let slots = slot_with_body(&mut ctx, "Body");
        let modal = modal(
            &mut ctx,
            ModalProps {
                title: "Confirm",
                ..Default::default()
            },
            slots,
        )
        .unwrap();
        let tree = ComponentList::new(modal);
        assert!(!find_text(&tree.commands(), "Confirm"));
    }
}
