use layout_core::{AlignItems, JustifyContent, LayoutError, LayoutStyle};
use reactive_core::RwSignal;
use renderer_core::{BorderRadius, Color, RectStyle, ShapeStyle, Stroke, TextStyle};
use ui_core::{Container, LayoutItem, Slots, StyledContainer, Text, box_item};

use crate::heading::heading_style;
use crate::scrim;
use crate::shared;

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
    pub title: Box<dyn Fn() -> String>,
    /// Runs after the modal sets `open = false` (scrim tap or Close), so a caller can react to dismissal.
    pub on_close: Option<Box<dyn Fn()>>,
    /// Dialog surface colour. `Color::TRANSPARENT` (the default) means "unset" -> `shared::DEFAULT_SURFACE`. A closure
    /// (re-read every frame) so a theme token or `$signal` colour re-colours live, like `button`'s `fill`.
    pub color: Box<dyn Fn() -> Color>,
}

impl Default for ModalProps {
    fn default() -> Self {
        Self {
            open: None,
            title: Box::new(String::new),
            on_close: None,
            color: Box::new(|| Color::TRANSPARENT),
        }
    }
}

pub fn modal(props: ModalProps, mut slots: Slots) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let ModalProps {
        open,
        title,
        on_close,
        color,
    } = props;
    let body = slots.take_default();

    scrim::scrim_overlay(open, on_close, move |dismiss| {
        build_open_modal(title, body, color, dismiss)
    })
}

/// Builds the scrim + centred opaque card for the open state: scrim (dims + dismisses) > centred card (title
/// row with Close, then the body). The card swallows its own taps so a click inside it never dismisses.
fn build_open_modal(
    title: Box<dyn Fn() -> String>,
    body: Vec<Box<dyn LayoutItem>>,
    color: Box<dyn Fn() -> Color>,
    dismiss: scrim::DismissFn,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    // `auto` (measured) so the title/Close get their intrinsic WIDTH in the header row; a plain `Text::new`
    // only stretches its cross-axis (height in a row), leaving width 0 and the text invisible.
    let heading = Text::auto(move || title(), LayoutStyle::new(), heading_style)?;

    let close_label = Text::auto(
        || "Close".to_string(),
        LayoutStyle::new(),
        || TextStyle::new(CLOSE_SIZE, CLOSE_INK),
    )?;
    let close = StyledContainer::new(
        LayoutStyle::new().flex_row(),
        |_r| RectStyle::default(),
        vec![box_item(close_label)],
    )?
    .on_press({
        let dismiss = dismiss.clone();
        move || (*dismiss)()
    });

    let header = Container::new(
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
                .with_fill(shared::resolve(color.as_ref(), || shared::DEFAULT_SURFACE))
                .with_stroke(Stroke::new(scrim::DEFAULT_BORDER, 1.0))
                .with_radius(BorderRadius::all(DIALOG_RADIUS))
        },
        dialog_children,
    )?
    // Swallow taps on the card so only the scrim (or Close) dismisses.
    .on_press(|| {});

    let scrim = StyledContainer::new(
        LayoutStyle::new()
            .flex_column()
            .flex_grow(1.0)
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER)
            .padding_all(DIALOG_PAD),
        |_r| RectStyle::default().with_fill(scrim::SCRIM),
        vec![box_item(dialog)],
    )?
    .on_press(move || (*dismiss)());

    Ok(box_item(scrim))
}

#[cfg(test)]
mod tests {
    use super::*;
    use layout_core::AvailableSpace;
    use reactive_core::signal;
    use renderer_core::DrawCommand;
    use ui_core::reset_layout_runtime;
    use ui_core::{ComponentList, compute_layout, new_container, relayout_if_dirty};

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

    // Toggling `open` shows then hides the dialog: the title and body are composed only while open, and the
    // Overlay portal is disposed when it closes (its content leaves the command stream).
    #[test]
    fn open_shows_dialog_and_close_hides_it() {
        reset_layout_runtime();
        let open = signal(false);
        let slots = slot_with_body("Body");
        let modal = modal(
            ModalProps {
                open: Some(open.clone()),
                title: Box::new(|| "Confirm".to_string()),
                ..Default::default()
            },
            slots,
        )
        .unwrap();

        // A parent-less root computed against the window registers the overlay host the portal attaches to.
        let root = new_container(
            LayoutStyle::new().flex_column().width(400.0).height(400.0),
            &[modal.layout_node()],
        )
        .unwrap();
        compute_layout(
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

    // A real modal must track the dismiss stack through the whole open/dismiss/reopen cycle, so Escape and a
    // Back gesture close it. This is the seam the raw-stack unit tests cannot reach: the registration effect is
    // created inside the `ReactiveList` build closure — an effect nested in a running effect.
    #[test]
    fn open_registers_on_the_dismiss_stack_and_dismissing_closes_it() {
        reset_layout_runtime();
        let open = signal(false);
        let modal = modal(
            ModalProps {
                open: Some(open.clone()),
                title: Box::new(|| "Confirm".to_string()),
                ..Default::default()
            },
            slot_with_body("Body"),
        )
        .unwrap();
        let root = new_container(
            LayoutStyle::new().flex_column().width(400.0).height(400.0),
            &[modal.layout_node()],
        )
        .unwrap();
        compute_layout(
            root,
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(400.0),
        )
        .unwrap();
        let _tree = ComponentList::new(modal);
        let before = ui_core::dismiss_depth();

        open.set(true);
        relayout_if_dirty();
        assert_eq!(
            ui_core::dismiss_depth(),
            before + 1,
            "an open modal is on the dismiss stack"
        );

        // What Escape/Back ultimately call: it must drive the modal's own `open` back to false.
        assert!(ui_core::dismiss_top());
        relayout_if_dirty();
        assert!(!open.get(), "dismissing closed the modal");
        assert_eq!(
            ui_core::dismiss_depth(),
            before,
            "closing withdrew the registration"
        );

        // Closing by its own affordance must withdraw too, not leave a stale entry a later Escape would hit.
        open.set(true);
        relayout_if_dirty();
        assert_eq!(ui_core::dismiss_depth(), before + 1);
        open.set(false);
        relayout_if_dirty();
        assert_eq!(
            ui_core::dismiss_depth(),
            before,
            "a self-close withdraws its entry"
        );
    }

    // An unbound modal (no `open` signal) builds a 0-size node and never portals anything.
    #[test]
    fn unbound_modal_renders_nothing() {
        reset_layout_runtime();
        let slots = slot_with_body("Body");
        let modal = modal(
            ModalProps {
                title: Box::new(|| "Confirm".to_string()),
                ..Default::default()
            },
            slots,
        )
        .unwrap();
        let tree = ComponentList::new(modal);
        assert!(!find_text(&tree.commands(), "Confirm"));
    }
}
