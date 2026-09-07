use crate::context::reset_layout_runtime;
use layout_core::AvailableSpace;
use platform_core::{PointerButton, PointerSource};
use reactive_core::{RwSignal, signal};

use super::*;
use crate::ComponentList;

/// The other half of the same barrier, and what §1.1 of the audit actually asked for: while a modal is up, Tab must not walk out to the content behind the scrim. The pointer has been blocked there all along; the keyboard was not, because the tab order is a list and a list has no notion of in front or behind.
#[test]
fn a_modal_that_is_up_holds_tab_inside_itself() {
    use crate::{StyledContainer, focus};

    reset_layout_runtime();
    let behind = focus::next_id();
    focus::register_as(behind, focus::FocusKind::Widget);

    let below = focus::next_id();
    let inside = StyledContainer::new(
        LayoutStyle::new().width(50.0).height(20.0),
        |_r| renderer_core::RectStyle::default(),
        vec![],
    )
    .unwrap()
    .on_focus(|_| {});
    let _overlay =
        Overlay::toggleable(LayoutStyle::new(), vec![Box::new(inside)], || true).unwrap();
    let above = focus::next_id();

    focus::request(behind);
    focus::focus_next();
    let landed = focus::current().expect("something took focus");
    assert!(
        landed > below && landed < above,
        "Tab left the modal that is up and landed on the content behind it"
    );
}

/// `view` draws nothing while hidden and `content_rect` is an empty barrier, but the in-tree walk stayed open — so every key press still reached the children of a dialog that was shut. The settling events are the exception, or content hidden mid-gesture keeps a hover it has no way left to clear.
#[test]
fn a_hidden_overlay_does_not_take_the_keyboard() {
    use std::cell::Cell;
    use std::rc::Rc;

    reset_layout_runtime();
    let keys = Rc::new(Cell::new(0u32));
    let counted = keys.clone();
    let showing = signal(false);
    let flag = showing;
    let field = crate::StyledContainer::new(
        LayoutStyle::new().width(50.0).height(20.0),
        |_r| renderer_core::RectStyle::default(),
        vec![],
    )
    .unwrap()
    .on_key(move |_| counted.set(counted.get() + 1));
    let mut overlay = Overlay::toggleable(LayoutStyle::new(), vec![Box::new(field)], move || {
        flag.get()
    })
    .unwrap();

    let press = Event::KeyPressed {
        key: platform_core::Key::Char('a'),
        modifiers: platform_core::ModifiersState::default(),
    };
    overlay.on_event(&press);
    assert_eq!(keys.get(), 0, "a shut dialog takes no keys");

    showing.set(true);
    overlay.on_event(&press);
    assert_eq!(keys.get(), 1, "and takes them again once it is up");

    // Hidden mid-gesture: the settling events still get through, or the content keeps the look it had.
    showing.set(false);
    assert_eq!(
        overlay.on_event(&Event::CursorLeft),
        crate::EventResult::Ignored,
        "CursorLeft reaches the children (they simply had nothing to settle)"
    );
}

/// The pointer path already scopes itself to what is on screen: a kept-mounted overlay whose `visible` reads false is inert, an empty barrier that blocks nothing. The keyboard path does not, and the two disagreeing is the bug — a focusable joins the tab order when its widget is *built*, and `toggleable` builds its subtree once and keeps it mounted, so a field inside a dialog that is shut is still a Tab stop. Not merely reachable past a scrim, as first described: reachable when nothing is open at all.
///
/// Closed by naming a *node* rather than a set of ids: the overlay's children are constructed before the overlay that will host them, so it never learns which focusables are its own, and ancestry answers at the moment Tab is pressed instead.
#[test]
fn tab_does_not_walk_into_an_overlay_that_is_not_showing() {
    use crate::{StyledContainer, focus};

    reset_layout_runtime();
    let base = focus::next_id();
    focus::register_as(base, focus::FocusKind::Widget);

    // Ids allocated between the two markers belong to whatever the overlay built.
    let below = focus::next_id();
    let field = StyledContainer::new(
        LayoutStyle::new().width(50.0).height(20.0),
        |_r| renderer_core::RectStyle::default(),
        vec![],
    )
    .unwrap()
    .on_focus(|_| {});
    let _overlay =
        Overlay::toggleable(LayoutStyle::new(), vec![Box::new(field)], || false).unwrap();
    let above = focus::next_id();

    focus::request(base);
    focus::focus_next();
    let landed = focus::current().expect("something took focus");
    assert!(
        !(landed > below && landed < above),
        "Tab reached a focusable inside an overlay that is not showing"
    );
}

/// A panel is placed from its trigger and then kept on screen. Without the second half, a tooltip on the rightmost button of a toolbar is laid out past the window edge and its text wraps into a column — the shape of the bug, not a cosmetic offset.
#[test]
fn an_anchored_panel_shifts_and_flips_to_stay_on_screen() {
    let viewport = Rect::new(0.0, 0.0, 400.0, 300.0);
    let panel = Rect::new(0.0, 0.0, 120.0, 60.0);

    let trigger = Rect::new(100.0, 100.0, 40.0, 20.0);
    assert_eq!(
        anchor_translate(trigger, panel, Placement::Below, viewport),
        (100.0, 120.0 + ANCHOR_GAP)
    );

    let right = Rect::new(380.0, 100.0, 20.0, 20.0);
    let (dx, dy) = anchor_translate(right, panel, Placement::Below, viewport);
    assert_eq!((dx, dy), (400.0 - 120.0 - EDGE_MARGIN, 120.0 + ANCHOR_GAP));

    let low = Rect::new(100.0, 270.0, 40.0, 20.0);
    let (_, dy) = anchor_translate(low, panel, Placement::Below, viewport);
    assert_eq!(dy, 270.0 - 60.0 - ANCHOR_GAP, "opens upward instead");

    let tall = Rect::new(0.0, 0.0, 120.0, 400.0);
    let (_, dy) = anchor_translate(low, tall, Placement::Below, viewport);
    assert_eq!(dy, 0.0);
}

/// Beside the trigger, the panel centres on it and flips across it when its own side runs out — the same two rules the vertical placements follow, on the other axis. A control in a vertical rail has no room below it and all the room in the world beside it, which is why the sideways pair exists at all.
#[test]
fn a_panel_placed_beside_its_trigger_centres_on_it_and_flips_when_it_has_to() {
    let viewport = Rect::new(0.0, 0.0, 400.0, 300.0);
    let panel = Rect::new(0.0, 0.0, 120.0, 60.0);
    let trigger = Rect::new(200.0, 100.0, 40.0, 20.0);

    let (dx, dy) = anchor_translate(trigger, panel, Placement::End, viewport);
    assert_eq!(
        dx,
        240.0 + ANCHOR_GAP,
        "starts a gap past where the trigger ends"
    );
    assert_eq!(dy, 100.0 + (20.0 - 60.0) / 2.0, "centred on the trigger");

    let (dx, _) = anchor_translate(trigger, panel, Placement::Start, viewport);
    assert_eq!(
        dx,
        80.0 - ANCHOR_GAP,
        "ends a gap before the trigger starts"
    );

    let rail = Rect::new(4.0, 100.0, 40.0, 20.0);
    let (dx, _) = anchor_translate(rail, panel, Placement::Start, viewport);
    assert_eq!(dx, 44.0 + ANCHOR_GAP, "flipped to the trailing side");

    // A tooltip beside a 36px button was crisp and the same one under a 28px tab was blurred, from nothing but the half pixel each placement contributed.
    let odd = Rect::new(200.0, 100.0, 40.0, 21.0);
    let (dx, dy) = anchor_translate(odd, panel, Placement::End, viewport);
    assert_eq!((dx, dy), (dx.round(), dy.round()), "on the pixel grid");
}
use crate::container::Container;
use crate::context::compute_layout;

fn press(x: f64, y: f64) -> Event {
    Event::PointerPressed {
        x,
        y,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    }
}
fn release(x: f64, y: f64) -> Event {
    Event::PointerReleased {
        x,
        y,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    }
}

// Mirrors the runner: the overlay registry first, then the tree only if no overlay consumed the event.
fn route(tree: &mut ComponentList, event: &Event) {
    if crate::dispatch_overlays(event) == EventResult::Ignored {
        tree.on_event(event);
    }
}

fn pressable(flag: RwSignal<bool>) -> Container {
    Container::new(LayoutStyle::new().width(400.0).height(400.0), vec![])
        .unwrap()
        .on_press(move || flag.set(true))
}

#[test]
fn background_alone_receives_tap() {
    reset_layout_runtime();
    let clicked = signal(false);
    let bg = pressable(clicked);
    let root = Container::new(
        LayoutStyle::new().flex_column().width(400.0).height(400.0),
        vec![Box::new(bg)],
    )
    .unwrap();
    let root_node = root.layout_node();
    compute_layout(
        root_node,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    let mut tree = ComponentList::new(root);
    let _ = tree.commands();

    route(&mut tree, &press(200.0, 200.0));
    route(&mut tree, &release(200.0, 200.0));
    assert!(
        clicked.get(),
        "background on_press must fire without an overlay"
    );
}

#[test]
fn overlay_receives_tap_and_blocks_background() {
    reset_layout_runtime();
    let bg_clicked = signal(false);
    let overlay_clicked = signal(false);

    let bg = pressable(bg_clicked);
    let scrim = Container::new(LayoutStyle::new().width(400.0).height(400.0), vec![])
        .unwrap()
        .on_press({
            let s = overlay_clicked;
            move || s.set(true)
        });
    let overlay = Overlay::new(LayoutStyle::new(), vec![Box::new(scrim)]).unwrap();
    let root = Container::new(
        LayoutStyle::new().flex_column().width(400.0).height(400.0),
        vec![Box::new(bg), Box::new(overlay)],
    )
    .unwrap();
    let root_node = root.layout_node();
    compute_layout(
        root_node,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    let mut tree = ComponentList::new(root);
    let _ = tree.commands();

    route(&mut tree, &press(200.0, 200.0));
    route(&mut tree, &release(200.0, 200.0));

    assert!(
        overlay_clicked.get(),
        "the tap must reach the overlay content"
    );
    assert!(
        !bg_clicked.get(),
        "the overlay must block the tap from the content behind it"
    );
}

// The portaled path, where `content_rect` is driven to the viewport by a later relayout: the page is laid out first (registering the host), and only then does the modal open. The test above covers the in-place fallback, where the overlay is built before any host exists.
#[test]
fn portaled_overlay_blocks_background() {
    use crate::context::relayout_if_dirty;

    reset_layout_runtime();
    let bg_clicked = signal(false);

    let bg = pressable(bg_clicked);
    let root = Container::new(
        LayoutStyle::new().flex_column().width(400.0).height(400.0),
        vec![Box::new(bg)],
    )
    .unwrap();
    let root_node = root.layout_node();
    compute_layout(
        root_node,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    let mut tree = ComponentList::new(root);
    let _ = tree.commands();

    let overlay_clicked = signal(false);
    let scrim = Container::new(LayoutStyle::new().width(400.0).height(400.0), vec![])
        .unwrap()
        .on_press({
            let s = overlay_clicked;
            move || s.set(true)
        });
    let _overlay = Overlay::new(LayoutStyle::new(), vec![Box::new(scrim)]).unwrap();
    relayout_if_dirty();

    route(&mut tree, &press(200.0, 200.0));
    route(&mut tree, &release(200.0, 200.0));

    assert!(
        overlay_clicked.get(),
        "the tap must reach the portaled overlay content"
    );
    assert!(
        !bg_clicked.get(),
        "the portaled overlay must block the tap from the page behind it"
    );
}

#[test]
fn click_through_overlay_lets_background_tap_through() {
    reset_layout_runtime();
    let bg_clicked = signal(false);
    let panel_clicked = signal(false);

    let bg = pressable(bg_clicked);
    let panel = Container::new(LayoutStyle::new().width(100.0).height(100.0), vec![])
        .unwrap()
        .on_press({
            let s = panel_clicked;
            move || s.set(true)
        });
    let overlay = Overlay::build(
        LayoutStyle::new(),
        vec![Box::new(panel)],
        false,
        None,
        Rc::new(|| true),
    )
    .unwrap();
    let root = Container::new(
        LayoutStyle::new().flex_column().width(400.0).height(400.0),
        vec![Box::new(bg), Box::new(overlay)],
    )
    .unwrap();
    let root_node = root.layout_node();
    compute_layout(
        root_node,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    let mut tree = ComponentList::new(root);
    let _ = tree.commands();

    route(&mut tree, &press(200.0, 200.0));
    route(&mut tree, &release(200.0, 200.0));
    assert!(
        bg_clicked.get(),
        "a tap on the transparent area must reach the background"
    );
    assert!(
        !panel_clicked.get(),
        "the panel must not receive a tap outside it"
    );

    bg_clicked.set(false);
    route(&mut tree, &press(50.0, 50.0));
    route(&mut tree, &release(50.0, 50.0));
    assert!(panel_clicked.get(), "a tap on the panel must reach it");
    assert!(
        !bg_clicked.get(),
        "the panel must block the tap from the background"
    );
}

#[test]
fn anchored_content_tracks_trigger() {
    use crate::context::relayout_if_dirty;

    reset_layout_runtime();

    let root = Container::new(
        LayoutStyle::new().flex_column().width(400.0).height(400.0),
        vec![],
    )
    .unwrap();
    let root_node = root.layout_node();
    compute_layout(
        root_node,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    let tree = ComponentList::new(root);
    let _ = tree.commands();

    let trigger = signal(Rect::new(50.0, 20.0, 80.0, 30.0));
    let panel = Container::new(LayoutStyle::new().width(120.0).height(60.0), vec![]).unwrap();
    let overlay = Overlay::build(
        LayoutStyle::new(),
        vec![Box::new(panel)],
        true,
        Some(Anchor {
            trigger: trigger,
            placement: Placement::Below,
        }),
        Rc::new(|| true),
    )
    .unwrap();
    relayout_if_dirty();

    let rect = overlay.anchored_barrier();
    assert_eq!((rect.x, rect.y), (50.0, 50.0 + ANCHOR_GAP));
    assert_eq!((rect.width, rect.height), (120.0, 60.0));

    trigger.set(Rect::new(200.0, 100.0, 80.0, 30.0));
    let rect = overlay.anchored_barrier();
    assert_eq!((rect.x, rect.y), (200.0, 130.0 + ANCHOR_GAP));
    assert_eq!((rect.width, rect.height), (120.0, 60.0));
}
