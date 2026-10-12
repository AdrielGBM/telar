use super::*;
use telar::{
    AvailableSpace, ComponentList, DrawCommand, compute_layout, new_container, relayout_if_dirty,
    signal,
};

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
    let before = telar::dismiss_depth();

    open.set(true);
    relayout_if_dirty();
    assert_eq!(
        telar::dismiss_depth(),
        before + 1,
        "an open modal is on the dismiss stack"
    );

    assert!(
        telar::dismiss_top(),
        "the open modal is what the dismiss stack pops"
    );
    relayout_if_dirty();
    assert!(!open.get(), "dismissing closed the modal");
    assert_eq!(
        telar::dismiss_depth(),
        before,
        "closing withdrew the registration"
    );

    open.set(true);
    relayout_if_dirty();
    assert_eq!(telar::dismiss_depth(), before + 1);
    open.set(false);
    relayout_if_dirty();
    assert_eq!(
        telar::dismiss_depth(),
        before,
        "a self-close withdraws its entry"
    );
}

// A named modal has nothing holding its state, so opening it before it is built must still bring it up: the name resolves to one shared signal either way.
#[test]
fn a_named_modal_opens_from_anywhere_even_before_it_is_built() {
    crate::test_support::fresh_layout_runtime();
    telar::close_overlay("confirm-test");
    telar::open_overlay("confirm-test");

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
        telar::dismiss_top(),
        "the named modal is on the dismiss stack"
    );
    relayout_if_dirty();
    assert!(
        !telar::overlay_state("confirm-test").peek(),
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
    telar::open_overlay("ignored-name");
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
    telar::close_overlay("ignored-name");
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

#[test]
fn a_reader_meets_the_dialog_under_its_title() {
    crate::test_support::fresh_layout_runtime();
    let open = signal(true);
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
    let tree = ComponentList::new(modal);
    relayout_if_dirty();

    let nodes = ui_core::accessibility::snapshot(&tree.commands());
    let dialogs: Vec<&str> = nodes
        .iter()
        .filter(|node| node.role == Role::Dialog)
        .map(|node| node.name.as_str())
        .collect();
    assert_eq!(dialogs, ["Confirm"]);
}

/// A button outside the dialog that held the keyboard before it opened, and a dialog bound to `open`.
fn opener_and_dialog(open: telar::RwSignal<bool>) -> (ComponentList, telar::focus::FocusId) {
    crate::test_support::fresh_layout_runtime();
    telar::focus::clear();
    let opener = StyledContainer::new(
        LayoutStyle::new().width(80.0).height(30.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .on_press(|| {})
    .control(Role::Button);
    let opener_id = opener.focus_id().expect("a control");
    let dialog = modal(
        ModalProps::props().open(open).title("Confirm").build(),
        Children::from(slot_with_body("Body")),
    )
    .unwrap();
    let root = Container::new(
        LayoutStyle::new().flex_column().width(400.0).height(400.0),
        vec![box_item(opener), dialog],
    )
    .unwrap();
    compute_layout(
        root.layout_node(),
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    let tree = ComponentList::new(root);
    telar::focus::request_from_pointer(opener_id);
    (tree, opener_id)
}

/// Closing a dialog gives the keyboard back to what opened it, as it held it: a button pressed open gets no ring.
#[test]
fn closing_gives_the_keyboard_back_to_the_opener() {
    let open = signal(false);
    let (tree, opener) = opener_and_dialog(open);
    open.set(true);
    relayout_if_dirty();
    let _ = tree.commands();
    assert_ne!(telar::focus::current(), Some(opener), "into the dialog");

    open.set(false);
    relayout_if_dirty();
    assert_eq!(telar::focus::current(), Some(opener));
    assert!(!telar::focus::is_focus_visible(opener));
}

/// A dialog whose action sent the keyboard somewhere else on its way out leaves it there.
#[test]
fn closing_leaves_the_keyboard_where_the_dialog_sent_it() {
    let open = signal(false);
    let (tree, _opener) = opener_and_dialog(open);
    let elsewhere = telar::focus::next_id();
    telar::focus::register_as(elsewhere, telar::focus::FocusKind::Widget);
    open.set(true);
    relayout_if_dirty();
    let _ = tree.commands();

    telar::focus::request(elsewhere);
    open.set(false);
    relayout_if_dirty();
    assert_eq!(telar::focus::current(), Some(elsewhere));
    telar::focus::unregister(elsewhere);
}

/// An open modal in a 400×400 window, built after the caller installed its theme.
fn open_modal(props: ModalProps) -> ComponentList {
    crate::test_support::fresh_layout_runtime();
    let modal = modal(props, Children::from(slot_with_body("Body"))).unwrap();
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
    tree
}

/// The dialog card's paint, found where a reader is told the dialog is.
fn card_paint(tree: &ComponentList) -> RectStyle {
    let nodes = ui_core::accessibility::snapshot(&tree.commands());
    let dialog = nodes
        .iter()
        .find(|node| node.role == Role::Dialog)
        .expect("the modal is open");
    crate::test_support::painted_at(tree, dialog.rect)
}

#[test]
fn the_card_is_rounded_by_the_dialog_role_until_the_tokens_name_a_radius() {
    crate::test_support::assert_panel_corner_follows_its_role(
        crate::test_support::PanelRole::Dialog,
        || {
            open_modal(
                ModalProps::props()
                    .open(signal(true))
                    .title("Confirm")
                    .build(),
            )
        },
        |tree| card_paint(tree).radius,
        BorderRadius::all,
        |radius| ModalTokens {
            radius: Some(radius),
            ..ModalTokens::default()
        },
    );
}

#[test]
fn the_tokens_size_and_pad_the_card_at_the_control_size_in_force() {
    let tokens = ModalTokens {
        pad: Some(10.0),
        min_width: Some(200.0),
        max_width: Some(300.0),
        ..ModalTokens::default()
    };
    telar::set_control_size(telar::ControlSize::Small);
    let small = (tokens.pad(), tokens.min_width(), tokens.max_width());
    telar::set_control_size(telar::ControlSize::Regular);
    let scale = telar::ControlSize::Small.scale();
    assert_eq!(
        small,
        (
            10.0 * scale,
            (200.0 * scale).round(),
            (300.0 * scale).round()
        )
    );

    let unset = ModalTokens::default();
    assert_eq!(
        unset.pad(),
        crate::metrics::gap_xl(),
        "an unset pad is the extra-large gap"
    );
    assert!(
        unset.min_width() < unset.max_width(),
        "the measured defaults leave the card room to grow"
    );
}

#[test]
fn a_style_amendment_restyles_the_card_over_its_own_paint() {
    let ink = Color::rgb(0.9, 0.1, 0.4);
    let tree = open_modal(
        ModalProps::props()
            .open(signal(true))
            .title("Confirm")
            .style(Rc::new(move |paint: RectStyle| {
                paint.with_fill(ink).with_radius(BorderRadius::all(0.0))
            }) as telar::SurfaceAmendment)
            .build(),
    );
    let paint = card_paint(&tree);
    assert_eq!(paint.fill, Some(ink.into()));
    assert_eq!(paint.radius, BorderRadius::all(0.0));
    assert!(
        paint.border.is_some(),
        "what the amendment left alone is the modal's own"
    );
}

#[derive(Clone)]
struct Dimmed(Color);

impl telar::ThemeTokens for Dimmed {
    fn overlay(&self) -> Color {
        self.0
    }
}

fn scrim_drawn(tree: &ComponentList, overlay: Color) -> bool {
    tree.commands().iter().any(|command| {
        matches!(command, DrawCommand::Rect { style, .. } if style.fill == Some(overlay.into()))
    })
}

#[test]
fn the_scrim_is_the_theme_overlay_and_follows_a_switch() {
    let first = Color::rgba(0.1, 0.0, 0.2, 0.4);
    let second = Color::rgba(0.0, 0.3, 0.0, 0.7);
    telar::set_theme(Dimmed(first));
    let tree = open_modal(
        ModalProps::props()
            .open(signal(true))
            .title("Confirm")
            .build(),
    );
    assert!(
        scrim_drawn(&tree, first),
        "the scrim is the theme's overlay"
    );

    telar::set_theme(Dimmed(second));
    assert!(
        scrim_drawn(&tree, second),
        "and is read again when it paints"
    );
    assert!(!scrim_drawn(&tree, first));
}

fn open_modal_with(body: impl FnOnce() -> Slots, window: (f32, f32)) -> ComponentList {
    crate::test_support::fresh_layout_runtime();
    let modal = modal(
        ModalProps::props()
            .open(signal(true))
            .title("Confirm")
            .build(),
        Children::from(body()),
    )
    .unwrap();
    let root = new_container(
        LayoutStyle::new()
            .flex_column()
            .width(window.0)
            .height(window.1),
        &[modal.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(window.0),
        AvailableSpace::Definite(window.1),
    )
    .unwrap();
    let tree = ComponentList::new(modal);
    relayout_if_dirty();
    tree
}

fn tall_body(lines: usize) -> Slots {
    let mut slots = Slots::new();
    for line in 0..lines {
        let text = Text::declaring(
            move || format!("Line {line}"),
            LayoutStyle::new().height(20.0),
            |t| t,
        )
        .unwrap();
        slots.push(None, box_item(text));
    }
    slots
}

fn card_rect(tree: &ComponentList) -> telar::Rect {
    ui_core::accessibility::snapshot(&tree.commands())
        .iter()
        .find(|node| node.role == Role::Dialog)
        .expect("the modal is open")
        .rect
}

fn plain_theme(tokens: Option<ModalTokens>) {
    telar::set_theme(crate::test_support::PanelTheme {
        role: crate::test_support::PanelRole::Dialog,
        radius: 9.0,
        tokens,
    });
}

#[test]
fn an_unset_width_is_the_tokens_and_the_window_bounds_it() {
    plain_theme(Some(ModalTokens {
        pad: Some(20.0),
        min_width: Some(250.0),
        max_width: Some(250.0),
        ..ModalTokens::default()
    }));
    let tree = open_modal_with(|| slot_with_body("Body"), (800.0, 400.0));
    assert_eq!(card_rect(&tree).width, 250.0, "the tokens size the card");

    let tree = open_modal_with(|| slot_with_body("Body"), (240.0, 400.0));
    let card = card_rect(&tree);
    assert_eq!(card.width, 200.0, "the window less the tokens' pad wins");
    assert_eq!(
        card.x, 20.0,
        "the pad is what the card stands off the edge by"
    );
}

#[test]
fn a_narrow_window_never_overflows_the_card() {
    plain_theme(None);
    let pad = ModalTokens::default().pad();
    for width in [120.0, 200.0, 300.0, 368.0] {
        let tree = open_modal_with(|| slot_with_body("Body"), (width, 400.0));
        let card = card_rect(&tree);
        assert!(card.x >= 0.0, "left edge at {width}: {card:?}");
        assert!(
            card.x + card.width <= width,
            "right edge at {width}: {card:?}"
        );
        assert!(card.width <= (width - pad * 2.0).max(0.0) + 0.5);
    }
}

#[test]
fn a_tall_body_scrolls_inside_a_card_that_fits_the_window() {
    plain_theme(None);
    let pad = ModalTokens::default().pad();
    let tree = open_modal_with(|| tall_body(40), (600.0, 400.0));
    let card = card_rect(&tree);
    assert!(card.y >= 0.0 && card.y + card.height <= 400.0, "{card:?}");
    assert!(card.height <= 400.0 - pad * 2.0 + 0.5, "{card:?}");

    let close = crate::test_support::drawn(Role::Button);
    assert_eq!(close.len(), 1, "the Close button is on screen");
    assert!(
        close[0].y >= card.y && close[0].y + close[0].height <= card.y + card.height,
        "{close:?} {card:?}"
    );

    let stops = telar::focus::exposed()
        .into_iter()
        .filter(|exposed| exposed.role == Role::ScrollArea)
        .filter_map(|exposed| ui_core::visible_rect(exposed.node))
        .collect::<Vec<_>>();
    assert_eq!(
        stops.len(),
        1,
        "an overflowing body is a stop the keyboard can scroll"
    );
    assert!(stops[0].y >= card.y && stops[0].y + stops[0].height <= card.y + card.height);
}

#[test]
fn a_short_body_is_no_stop_of_its_own() {
    plain_theme(None);
    let _tree = open_modal_with(|| slot_with_body("Body"), (600.0, 400.0));
    assert!(
        telar::focus::exposed()
            .iter()
            .all(|exposed| exposed.role != Role::ScrollArea)
    );
}
