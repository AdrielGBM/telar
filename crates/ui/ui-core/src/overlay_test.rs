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
        unmodified: None,
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
    assert_eq!(dy, EDGE_MARGIN, "the roomier side, as far up as the margin");
}

fn trigger_at(y: f32) -> Rect {
    Rect::new(10.0, y, 100.0, 36.0)
}

fn window(height: f32) -> Rect {
    Rect::new(0.0, 0.0, 400.0, height)
}

fn below(anchor: Rect, viewport: Rect) -> Anchoring {
    Anchoring::new(anchor, Placement::Below, viewport)
}

#[test]
fn a_panel_that_fits_opens_below_at_its_size() {
    let spot = below(trigger_at(0.0), window(400.0)).place(Size::new(180.0, 100.0));
    assert_eq!(spot, Rect::new(10.0, 36.0, 180.0, 100.0));
}

/// A list longer than the window used to open at its full height and run off the bottom; capped to the room, it scrolls inside it instead.
#[test]
fn a_panel_taller_than_the_room_below_is_capped_to_it() {
    let spot = below(trigger_at(0.0), window(200.0)).place(Size::new(180.0, 800.0));
    assert_eq!(spot.y, 36.0);
    assert_eq!(spot.y + spot.height, 200.0 - EDGE_MARGIN);
}

#[test]
fn a_panel_flips_above_when_it_does_not_fit_below_and_there_is_more_room_there() {
    let spot = below(trigger_at(300.0), window(400.0)).place(Size::new(180.0, 250.0));
    assert_eq!(spot.height, 250.0);
    assert_eq!(
        spot.y + spot.height,
        300.0,
        "its bottom edge meets the trigger"
    );
}

#[test]
fn a_flipped_panel_is_capped_to_the_room_above() {
    let spot = below(trigger_at(300.0), window(400.0)).place(Size::new(180.0, 800.0));
    assert_eq!(spot.y, EDGE_MARGIN);
    assert_eq!(spot.y + spot.height, 300.0);
}

#[test]
fn a_panel_too_tall_for_either_side_stays_where_it_was_asked_when_that_side_is_roomier() {
    let spot = below(trigger_at(150.0), window(400.0)).place(Size::new(180.0, 800.0));
    assert_eq!(spot.y, 186.0);
    assert_eq!(spot.y + spot.height, 400.0 - EDGE_MARGIN);
}

/// The cap a panel is laid out under is the roomier side, and whatever it measures under it is placed without a second pass: it either fits where it was asked to open, or it is no taller than the side it flips to.
#[test]
fn the_room_is_the_roomier_side_and_what_fits_it_is_placed_in_full() {
    let anchoring = below(trigger_at(300.0), window(400.0));
    let room = anchoring.room();
    assert_eq!(room.height, 300.0 - EDGE_MARGIN);
    assert_eq!(room.width, 400.0 - EDGE_MARGIN * 2.0);
    for height in [20.0, 60.0, 61.0, 200.0, room.height] {
        let spot = anchoring.place(Size::new(180.0, height));
        assert_eq!(spot.height, height, "a panel of {height} is drawn whole");
    }
}

/// A context menu opens at a point, which is an anchor of no size: below and after it, or above it when the room is there.
#[test]
fn a_point_anchor_opens_below_it_and_flips_above_it() {
    let viewport = window(300.0);
    let point = Rect::new(40.0, 30.0, 0.0, 0.0);
    let spot = below(point, viewport).place(Size::new(120.0, 100.0));
    assert_eq!((spot.x, spot.y), (40.0, 30.0));

    let low = Rect::new(390.0, 280.0, 0.0, 0.0);
    let spot = below(low, viewport).place(Size::new(120.0, 100.0));
    assert_eq!(spot.y + spot.height, 280.0, "above the point");
    assert_eq!(
        spot.x,
        400.0 - 120.0 - EDGE_MARGIN,
        "and back inside the window"
    );
}

/// A submenu opens beside its row and level with it, rather than centred on it.
#[test]
fn a_panel_beside_its_anchor_can_line_up_with_its_start() {
    let row = Rect::new(100.0, 250.0, 120.0, 30.0);
    let spot = Anchoring::new(row, Placement::End, window(300.0))
        .align(AnchorAlign::Start)
        .place(Size::new(120.0, 30.0));
    assert_eq!((spot.x, spot.y), (220.0, 250.0));

    let spot = Anchoring::new(row, Placement::End, window(300.0))
        .align(AnchorAlign::Start)
        .place(Size::new(120.0, 100.0));
    assert_eq!(
        spot.y + spot.height,
        300.0 - EDGE_MARGIN,
        "shifted up to stay on screen"
    );
}

/// The gap between a panel and its anchor is kept on whichever side the panel ends up: a gap below a trigger it drops from, a gap above one it flips over, and the room on each side is what is left once the gap is kept.
#[test]
fn the_gap_is_kept_on_the_side_the_panel_opens_on() {
    let gap = 8.0;
    let spot = below(trigger_at(0.0), window(400.0))
        .gap(gap)
        .place(Size::new(180.0, 100.0));
    assert_eq!(spot.y, 36.0 + gap, "a gap under the trigger");

    let flipped = below(trigger_at(300.0), window(400.0)).gap(gap);
    assert_eq!(flipped.room().height, 300.0 - gap - EDGE_MARGIN);
    let spot = flipped.place(Size::new(180.0, 250.0));
    assert_eq!(spot.y + spot.height, 300.0 - gap, "a gap over the trigger");

    let spot = flipped.place(Size::new(180.0, 60.0));
    assert_eq!(
        spot.y + spot.height,
        300.0 - gap,
        "sixty fit below the trigger only without the gap, so it flips"
    );
    let spot = below(trigger_at(300.0), window(400.0)).place(Size::new(180.0, 60.0));
    assert_eq!(spot.y, 336.0, "and with none it stays below");
}

#[test]
fn an_unbounded_viewport_never_constrains() {
    let unbounded = Rect::new(0.0, 0.0, f32::MAX, f32::MAX);
    let spot = below(trigger_at(0.0), unbounded).place(Size::new(180.0, 800.0));
    assert_eq!(spot, Rect::new(10.0, 36.0, 180.0, 800.0));
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

// The portaled path, where `content_rect` is driven to the viewport by a later relayout: the page is laid out first (registering the host), and only then does the modal open. The test above covers an overlay built before any host exists, which waits and is attached on the first layout pass.
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

    let (node, trigger) = crate::context::new_leaf(LayoutStyle::new()).unwrap();
    trigger.set(Rect::new(50.0, 20.0, 80.0, 30.0));
    let panel = Container::new(LayoutStyle::new().width(120.0).height(60.0), vec![]).unwrap();
    let overlay = Overlay::build(
        LayoutStyle::new(),
        vec![Box::new(panel)],
        true,
        Some(Anchor {
            trigger: node,
            laid: trigger,
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

fn fills_in_order(
    commands: &[renderer_core::DrawCommand],
    color: renderer_core::Color,
) -> Vec<(usize, Rect)> {
    commands
        .iter()
        .enumerate()
        .filter_map(|(at, command)| match command {
            renderer_core::DrawCommand::Rect { rect, style, .. }
                if style.fill == Some(color.into()) =>
            {
                Some((at, *rect))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn a_modal_open_from_the_start_covers_the_window_above_the_tree() {
    use crate::{StyledContainer, WindowRoot, relayout_if_dirty};
    use renderer_core::{Color, RectStyle, ShapeStyle};

    reset_layout_runtime();
    let swatch = |color: Color, style: LayoutStyle| {
        StyledContainer::new(
            style,
            move |_| RectStyle::default().with_fill(color),
            vec![],
        )
        .unwrap()
    };
    let fill = || {
        LayoutStyle::new()
            .width(layout_core::SizeDimension::Percent(1.0))
            .height(layout_core::SizeDimension::Percent(1.0))
    };
    let scrim = swatch(Color::BLACK, fill());
    let modal = Overlay::toggleable(LayoutStyle::new(), vec![Box::new(scrim)], || true).unwrap();
    let declared_in = Container::new(
        LayoutStyle::new().width(120.0).height(60.0),
        vec![Box::new(modal)],
    )
    .unwrap();
    let page = Container::new(
        LayoutStyle::new().flex_column().flex_grow(1.0),
        vec![
            Box::new(swatch(Color::WHITE, LayoutStyle::new().height(40.0))),
            Box::new(declared_in),
        ],
    )
    .unwrap();
    crate::set_surface_size(geometry_core::Size::new(400.0, 300.0));
    let mut tree = ComponentList::new(WindowRoot::wrapping(Box::new(page)).unwrap());
    tree.on_event(&Event::WindowResized {
        width: 400,
        height: 300,
    });
    relayout_if_dirty();

    let commands = tree.commands().clone();
    let scrims = fills_in_order(&commands, Color::BLACK);
    assert_eq!(
        scrims.iter().map(|(_, rect)| *rect).collect::<Vec<_>>(),
        [Rect::new(0.0, 0.0, 400.0, 300.0)],
        "the scrim spans the window, not the 120×60 box it was declared in"
    );
    let (page_at, _) = fills_in_order(&commands, Color::WHITE)[0];
    assert!(scrims[0].0 > page_at, "the modal is drawn above the tree");
    assert_eq!(
        crate::dispatch_overlays(&press(350.0, 250.0)),
        EventResult::Handled,
        "and blocks the whole window, far from where it was declared"
    );
}

#[test]
fn dropping_a_portaled_overlay_leaves_its_content_for_the_placeholder_that_holds_it() {
    use crate::context::{live_node_count, relayout_if_dirty, remove_node, track_layout};

    reset_layout_runtime();
    let host = Container::new(LayoutStyle::new().width(400.0).height(400.0), vec![]).unwrap();
    compute_layout(
        host.layout_node(),
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    let baseline = live_node_count();
    let overlay = Overlay::new(LayoutStyle::new(), vec![]).unwrap();
    let content = overlay.content_node();
    let placeholder = overlay.layout_node();
    assert_ne!(content, placeholder, "the content is portaled");
    relayout_if_dirty();

    drop(overlay);
    assert!(
        track_layout(content).is_some(),
        "the content outlives the widget until its owner is disposed"
    );

    remove_node(placeholder);
    assert!(
        track_layout(content).is_none(),
        "freed with its placeholder"
    );
    assert_eq!(live_node_count(), baseline);
}

/// The host is not the overlay's, and outlives the tree the overlay was declared in: still hanging from it, the content was laid out on the next pass with its rect signals freed by the owner, and the pass panicked.
#[test]
fn disposing_its_owner_takes_a_portaled_overlays_content_off_a_host_that_outlives_it() {
    use crate::context::relayout_if_dirty;
    use layout_reactive::children;

    reset_layout_runtime();
    let host = Container::new(LayoutStyle::new().width(400.0).height(400.0), vec![]).unwrap();
    let host_node = host.layout_node();
    compute_layout(
        host_node,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    let scope = reactive_core::owner_scope();
    let panel = Container::new(LayoutStyle::new().width(50.0).height(20.0), vec![]).unwrap();
    let overlay = Overlay::new(LayoutStyle::new(), vec![Box::new(panel)]).unwrap();
    let content = overlay.content_node();
    let tree = ComponentList::new(
        Container::new(LayoutStyle::new().flex_column(), vec![Box::new(overlay)]).unwrap(),
    );
    let owner = scope.id();
    drop(scope);
    relayout_if_dirty();
    assert!(
        children(host_node).contains(&content),
        "portaled to the host"
    );

    drop(tree);
    reactive_core::dispose_owner(owner);
    assert!(
        !children(host_node).contains(&content),
        "taken off the host with its owner"
    );
    compute_layout(
        host_node,
        AvailableSpace::Definite(500.0),
        AvailableSpace::Definite(500.0),
    )
    .expect("the host lays out without the content");
}

/// Its content is a root of its own, so a box hiding the place it was declared in has to tell it, as it tells a fixed layer: an open overlay is hidden with that box rather than closed, and back when the box is.
#[test]
fn a_box_hiding_where_the_overlay_was_declared_hides_the_overlay() {
    use crate::{Container, StyledContainer};
    use renderer_core::{DrawCommand, ElementId};
    use ui_tree::SegmentRoot;

    reset_layout_runtime();
    let panel = StyledContainer::new(
        LayoutStyle::new().width(50.0).height(20.0),
        |_r| renderer_core::RectStyle::default(),
        vec![],
    )
    .unwrap();
    let panel_node = panel.layout_node();
    let overlay = Overlay::new(LayoutStyle::new(), vec![Box::new(panel)]).unwrap();
    let section =
        Container::new(LayoutStyle::new().flex_column(), vec![Box::new(overlay)]).unwrap();
    let section_node = section.layout_node();
    let root = SegmentRoot::mount(section);
    let drawn = |root: &SegmentRoot| {
        let id = ElementId(panel_node.into());
        root.commands().iter().any(
            |command| matches!(command, DrawCommand::PushElement { element } if element.id == id),
        )
    };
    assert!(drawn(&root));

    crate::context::set_layout_style(section_node, LayoutStyle::new().flex_column().shown(false))
        .unwrap();
    assert!(!drawn(&root), "hidden with the section it was declared in");

    crate::context::set_layout_style(section_node, LayoutStyle::new().flex_column().shown(true))
        .unwrap();
    assert!(drawn(&root), "and back when the section is");
}

/// The boxes a modal overlay is made of say so, naming the layer and, once it is given, the control it was opened from: what lets a document make the rest of the page inert. An overlay that takes no pointer says nothing.
#[test]
fn a_modal_overlay_marks_its_boxes_and_names_what_opened_it() {
    use crate::{Container, StyledContainer};
    use renderer_core::{DrawCommand, ElementId, Modal};
    use ui_tree::SegmentRoot;

    reset_layout_runtime();
    let was = ui_tree::set_element_capture(true);
    let boxed = || {
        StyledContainer::new(
            LayoutStyle::new().width(50.0).height(20.0),
            |_r| renderer_core::RectStyle::default(),
            vec![],
        )
        .unwrap()
    };
    let trigger = boxed();
    let trigger_node = trigger.layout_node();
    let (first, second, tip) = (boxed(), boxed(), boxed());
    let (first_node, second_node, tip_node) =
        (first.layout_node(), second.layout_node(), tip.layout_node());
    let menu = Overlay::new(LayoutStyle::new(), vec![Box::new(first), Box::new(second)])
        .unwrap()
        .owned_by(trigger_node);
    let tooltip = Overlay::anchored_click_through(
        LayoutStyle::new(),
        vec![Box::new(tip)],
        trigger_node,
        Placement::Below,
    )
    .unwrap();
    let section = Container::new(
        LayoutStyle::new().flex_column(),
        vec![Box::new(trigger), Box::new(menu), Box::new(tooltip)],
    )
    .unwrap();
    let root = SegmentRoot::mount(section);
    let modal_of = |node: NodeId| {
        let id = ElementId(node.into());
        root.commands()
            .iter()
            .find_map(|command| match command {
                DrawCommand::PushElement { element } if element.id == id => {
                    Some(element.semantics.modal)
                }
                _ => None,
            })
            .expect("the box is drawn")
    };
    let layer = Some(Modal {
        layer: first_node.into(),
        opened_from: Some(trigger_node.into()),
    });
    assert_eq!(modal_of(first_node), layer);
    assert_eq!(
        modal_of(second_node),
        layer,
        "one layer, however many boxes"
    );
    assert_eq!(modal_of(trigger_node), None);
    assert_eq!(modal_of(tip_node), None);
    ui_tree::set_element_capture(was);
}
