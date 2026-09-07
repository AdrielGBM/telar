use super::*;
use layout_core::AvailableSpace;
use reactive_core::signal;
use renderer_core::DrawCommand;
use ui_core::{ComponentList, compute_layout, new_container, relayout_if_dirty};

fn find_text(cmds: &[DrawCommand], needle: &str) -> bool {
    cmds.iter()
        .any(|c| matches!(c, DrawCommand::Text { text, .. } if text.as_ref() == needle))
}

fn slot_with_body(label: &'static str) -> Slots {
    let body = Text::declaring(
        move || label.to_string(),
        LayoutStyle::new().height(20.0),
        |t| t,
    )
    .unwrap();
    let mut slots = Slots::new();
    slots.push(None, box_item(body));
    slots
}

#[test]
fn open_shows_dialog_and_close_hides_it() {
    crate::test_support::fresh_layout_runtime();
    let open = signal(false);
    let slots = slot_with_body("Body");
    let modal = modal(
        ModalProps::props().open(open).title("Confirm").build(),
        Children::from(slots),
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

    assert!(
        !find_text(&tree.commands(), "Confirm"),
        "a closed modal draws no title"
    );
    assert!(!find_text(&tree.commands(), "Body"), "nor its body");

    open.set(true);
    relayout_if_dirty();
    assert!(
        find_text(&tree.commands(), "Confirm"),
        "title shows when open"
    );
    assert!(find_text(&tree.commands(), "Body"), "body shows when open");

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

// The seam the raw-stack unit tests cannot reach: the registration effect is created inside the `ReactiveList` build closure, an effect nested in a running effect.
#[test]
fn open_registers_on_the_dismiss_stack_and_dismissing_closes_it() {
    crate::test_support::fresh_layout_runtime();
    let open = signal(false);
    let modal = modal(
        ModalProps::props().open(open).title("Confirm").build(),
        Children::from(slot_with_body("Body")),
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

    assert!(
        ui_core::dismiss_top(),
        "the open modal is what the dismiss stack pops"
    );
    relayout_if_dirty();
    assert!(!open.get(), "dismissing closed the modal");
    assert_eq!(
        ui_core::dismiss_depth(),
        before,
        "closing withdrew the registration"
    );

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

// A named modal has nothing holding its state, so opening it before it is built must still bring it up: the name resolves to one shared signal either way.
#[test]
fn a_named_modal_opens_from_anywhere_even_before_it_is_built() {
    crate::test_support::fresh_layout_runtime();
    ui_core::close_overlay("confirm-test");
    ui_core::open_overlay("confirm-test");

    let modal = modal(
        ModalProps::props()
            .id("confirm-test")
            .title("Confirm")
            .build(),
        Children::from(slot_with_body("Body")),
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
    let tree = ComponentList::new(modal);
    relayout_if_dirty();
    assert!(
        find_text(&tree.commands(), "Confirm"),
        "the dialog came up open, from a name opened before it existed"
    );

    assert!(
        ui_core::dismiss_top(),
        "the named modal is on the dismiss stack"
    );
    relayout_if_dirty();
    assert!(
        !ui_core::overlay_state("confirm-test").peek(),
        "dismissing clears its open state"
    );
    assert!(
        !find_text(&tree.commands(), "Confirm"),
        "so it draws nothing"
    );
}

// Given both, the explicitly bound signal is authoritative — otherwise the two states would race.
#[test]
fn an_explicit_open_signal_wins_over_a_name() {
    crate::test_support::fresh_layout_runtime();
    ui_core::open_overlay("ignored-name");
    let open = signal(false);
    let modal = modal(
        ModalProps::props()
            .open(open)
            .id("ignored-name")
            .title("Confirm")
            .build(),
        Children::from(slot_with_body("Body")),
    )
    .unwrap();
    let tree = ComponentList::new(modal);
    assert!(
        !find_text(&tree.commands(), "Confirm"),
        "the bound signal says closed, so the open name is ignored"
    );
    ui_core::close_overlay("ignored-name");
}

#[test]
fn unbound_modal_renders_nothing() {
    crate::test_support::fresh_layout_runtime();
    let slots = slot_with_body("Body");
    let modal = modal(
        ModalProps::props().title("Confirm").build(),
        Children::from(slots),
    )
    .unwrap();
    let tree = ComponentList::new(modal);
    assert!(
        !find_text(&tree.commands(), "Confirm"),
        "a modal bound to nothing draws nothing"
    );
}
