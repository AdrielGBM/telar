use std::cell::{Cell, RefCell};
use std::rc::Rc;

use layout_core::{AvailableSpace, LayoutStyle};
use platform_core::{Key, ModifiersState, PointerButton, PointerSource, ScrollDelta};
use reactive_core::signal;
use renderer_core::{Color, RectStyle, TextStyle};
use ui_tree::{Component, RenderNode};

use super::*;
use crate::canvas::Canvas;
use crate::container::Container;
use crate::context::{
    compute_layout, mark_dirty, new_container, reset_layout_runtime, set_children, set_display,
    set_overlay_host,
};
use crate::focus::{self, FocusKind};
use crate::input::Input;
use crate::layout_item::LayoutItem;
use crate::overlay::Overlay;
use crate::scroll_area::LayoutScrollArea;
use crate::styled_container::{StyledContainer, box_transform};
use crate::surface_context::Surface;

fn boxed(style: LayoutStyle, children: Vec<Box<dyn LayoutItem>>) -> StyledContainer {
    StyledContainer::new(style, |_r| RectStyle::default(), children).unwrap()
}

fn full() -> LayoutStyle {
    LayoutStyle::new().width(400.0).height(400.0)
}

fn rect_at(x: f32, y: f32, width: f32, height: f32) -> LayoutStyle {
    LayoutStyle::new()
        .absolute()
        .inset_start(x)
        .inset_top(y)
        .width(width)
        .height(height)
}

fn placed(x: f32, y: f32, size: f32) -> LayoutStyle {
    rect_at(x, y, size, size)
}

fn lay_out(root: &impl LayoutItem) {
    compute_layout(
        root.layout_node(),
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
}

fn counter() -> (Rc<Cell<u32>>, impl Fn() + 'static) {
    let count = Rc::new(Cell::new(0));
    let sink = count.clone();
    (count, move || sink.set(sink.get() + 1))
}

fn press_with(x: f64, y: f64, button: PointerButton) -> Event {
    Event::PointerPressed {
        x,
        y,
        button,
        source: PointerSource::Mouse,
    }
}

fn release_with(x: f64, y: f64, button: PointerButton) -> Event {
    Event::PointerReleased {
        x,
        y,
        button,
        source: PointerSource::Mouse,
    }
}

fn tap(root: &mut impl Component, x: f64, y: f64) -> EventResult {
    let pressed = root.on_event(&press_with(x, y, PointerButton::Primary));
    root.on_event(&release_with(x, y, PointerButton::Primary));
    pressed
}

fn tap_overlays(x: f64, y: f64) -> EventResult {
    let pressed = crate::dispatch_overlays(&press_with(x, y, PointerButton::Primary));
    crate::dispatch_overlays(&release_with(x, y, PointerButton::Primary));
    pressed
}

fn moved(x: f64, y: f64) -> Event {
    Event::PointerMoved {
        x,
        y,
        source: PointerSource::Mouse,
    }
}

fn claimed(x: f32, y: f32) -> bool {
    interactive_rects().iter().any(|rect| rect.contains(x, y))
}

/// Where a node can be pointed at, as the bounds around its drawn shape so a test can name one rect.
fn pointable_rect(node: NodeId) -> Option<Rect> {
    drawn(node, layout_reactive::track_layout(node)?.peek()).map(|shape| shape.bounds())
}

fn pane_style() -> LayoutStyle {
    rect_at(0.0, 50.0, 400.0, 350.0)
}

#[test]
fn a_lone_opaque_box_claims_exactly_its_rect() {
    reset_layout_runtime();
    let card = boxed(placed(20.0, 30.0, 100.0), vec![]).input_opaque();
    let root = Container::new(full(), vec![Box::new(card)]).unwrap();
    lay_out(&root);
    assert_eq!(
        interactive_rects(),
        vec![Rect::new(20.0, 30.0, 100.0, 100.0)]
    );
}

#[test]
fn an_opaque_box_keeps_the_press_from_everything_beneath_it() {
    reset_layout_runtime();
    let (pane_taps, pane_tap) = counter();
    let (outer_taps, outer_tap) = counter();
    let dragged = Rc::new(Cell::new(false));
    let drag_sink = dragged.clone();
    let pane = boxed(full(), vec![]).on_press(pane_tap);
    let card = boxed(placed(0.0, 0.0, 100.0), vec![]).input_opaque();
    let mut outer = boxed(full(), vec![Box::new(pane), Box::new(card)])
        .on_press(outer_tap)
        .on_drag(move |_x, _y| drag_sink.set(true));
    lay_out(&outer);

    assert_eq!(
        tap(&mut outer, 50.0, 50.0),
        EventResult::Handled,
        "the card takes the press"
    );
    assert_eq!(
        (pane_taps.get(), outer_taps.get(), dragged.get()),
        (0, 0, false),
        "and neither the pane beneath it nor the box around it hears of it"
    );
    tap(&mut outer, 200.0, 200.0);
    assert_eq!(
        pane_taps.get(),
        1,
        "beside the card the pane still takes a tap"
    );
}

#[test]
fn a_secondary_press_on_an_opaque_box_goes_no_further() {
    reset_layout_runtime();
    let heard = Rc::new(Cell::new(None));
    let sink = heard.clone();
    let pane = boxed(full(), vec![]).on_alt_press(move |button| sink.set(Some(button)));
    let card = boxed(placed(0.0, 0.0, 100.0), vec![]).input_opaque();
    let mut root = Container::new(full(), vec![Box::new(pane), Box::new(card)]).unwrap();
    lay_out(&root);

    assert_eq!(
        root.on_event(&press_with(50.0, 50.0, PointerButton::Secondary)),
        EventResult::Handled
    );
    root.on_event(&release_with(50.0, 50.0, PointerButton::Secondary));
    assert_eq!(
        heard.get(),
        None,
        "the pane under the card hears no right-click"
    );
}

#[test]
fn a_drag_that_starts_beside_an_opaque_box_ends_over_it() {
    reset_layout_runtime();
    let presses = Rc::new(Cell::new(0u32));
    let ends = Rc::new(Cell::new(0u32));
    let (press_sink, end_sink) = (presses.clone(), ends.clone());
    let pane = boxed(full(), vec![])
        .on_drag(move |_x, _y| press_sink.set(press_sink.get() + 1))
        .on_drag_end(move |_x, _y| end_sink.set(end_sink.get() + 1));
    let card = boxed(placed(200.0, 200.0, 100.0), vec![]).input_opaque();
    let mut root = Container::new(full(), vec![Box::new(pane), Box::new(card)]).unwrap();
    lay_out(&root);

    root.on_event(&press_with(250.0, 250.0, PointerButton::Primary));
    root.on_event(&release_with(250.0, 250.0, PointerButton::Primary));
    assert_eq!(
        (presses.get(), ends.get()),
        (0, 0),
        "a stroke started on the card is not the pane's drag"
    );

    root.on_event(&press_with(50.0, 50.0, PointerButton::Primary));
    root.on_event(&moved(250.0, 250.0));
    root.on_event(&release_with(250.0, 250.0, PointerButton::Primary));
    assert_eq!(
        ends.get(),
        1,
        "a release over the card still ends the drag it did not start"
    );
}

#[test]
fn hiding_an_opaque_box_takes_it_out_of_the_region_and_the_hit_test() {
    reset_layout_runtime();
    let (taps, on_tap) = counter();
    let card_rect = Rect::new(0.0, 0.0, 100.0, 100.0);
    let pane = boxed(pane_style(), vec![]).on_press(on_tap);
    let card = boxed(placed(0.0, 0.0, 100.0), vec![]).input_opaque();
    let card_node = card.layout_node();
    let mut root = Container::new(full(), vec![Box::new(pane), Box::new(card)]).unwrap();
    lay_out(&root);
    assert!(interactive_rects().contains(&card_rect));

    set_display(card_node, false);
    mark_dirty(card_node).unwrap();
    lay_out(&root);
    assert!(
        !interactive_rects().contains(&card_rect),
        "a hidden card claims nothing"
    );
    tap(&mut root, 50.0, 75.0);
    assert_eq!(
        taps.get(),
        1,
        "and the press reaches the pane it no longer covers"
    );

    set_display(card_node, true);
    mark_dirty(card_node).unwrap();
    lay_out(&root);
    assert!(
        interactive_rects().contains(&card_rect),
        "shown again, it claims its rect"
    );
    tap(&mut root, 50.0, 75.0);
    assert_eq!(taps.get(), 1, "and covers the pane again");
}

/// A disabled box keeps its claim, so it owes the press an ending.
///
/// The claim is right — the box is still drawn, and a click on it must not reach whatever the surface is over. What was wrong was declining the press afterwards: on a surface whose compositor input region is carved from these claims, nothing behind it is ever offered the click, so an unhandled press is a click that happened nowhere at all.
#[test]
fn a_disabled_box_consumes_the_press_it_claims() {
    reset_layout_runtime();
    let card_rect = Rect::new(0.0, 0.0, 100.0, 100.0);
    let (pane_taps, pane_tap) = counter();
    let (card_taps, card_tap) = counter();
    let pane = boxed(pane_style(), vec![]).on_press(pane_tap);
    let card = boxed(placed(0.0, 0.0, 100.0), vec![])
        .on_press(card_tap)
        .disabled(|| true);
    let mut root = Container::new(full(), vec![Box::new(pane), Box::new(card)]).unwrap();
    lay_out(&root);

    assert!(
        interactive_rects().contains(&card_rect),
        "a disabled control still claims its rect"
    );
    assert_eq!(
        tap(&mut root, 50.0, 75.0),
        EventResult::Handled,
        "so the press it claimed ends there"
    );
    assert_eq!(
        (card_taps.get(), pane_taps.get()),
        (0, 0),
        "with neither it nor the pane it covers acting on it"
    );
    tap(&mut root, 50.0, 200.0);
    assert_eq!(
        pane_taps.get(),
        1,
        "past the card the pane still takes a tap"
    );
}

#[test]
fn a_box_faded_to_nothing_claims_nothing_and_covers_nothing() {
    reset_layout_runtime();
    let (taps, on_tap) = counter();
    let card_rect = Rect::new(0.0, 0.0, 100.0, 100.0);
    let opacity = signal(0.0f32);
    let pane = boxed(pane_style(), vec![]).on_press(on_tap);
    let card = boxed(placed(0.0, 0.0, 100.0), vec![])
        .with_opacity(move || opacity.get())
        .input_opaque();
    let mut root = Container::new(full(), vec![Box::new(pane), Box::new(card)]).unwrap();
    lay_out(&root);

    assert!(
        !interactive_rects().contains(&card_rect),
        "a card at opacity 0 claims nothing"
    );
    tap(&mut root, 50.0, 75.0);
    assert_eq!(taps.get(), 1, "and the pane beneath it takes the press");

    opacity.set(1.0);
    assert!(
        interactive_rects().contains(&card_rect),
        "faded in, it claims its rect"
    );
    tap(&mut root, 50.0, 75.0);
    assert_eq!(taps.get(), 1, "and covers the pane");
}

#[test]
fn fading_to_nothing_lets_go_of_the_hover_and_the_focus_inside() {
    reset_layout_runtime();
    focus::clear();
    let level = signal(1.0f32);
    let hovered = Rc::new(Cell::new(false));
    let sink = hovered.clone();
    let field = Input::new(
        signal(String::new()),
        LayoutStyle::new().width(100.0).height(20.0),
        || TextStyle::new(14.0, Color::BLACK),
    )
    .unwrap()
    .autofocus();
    let mut faded = boxed(full(), vec![Box::new(field)])
        .with_opacity(move || level.get())
        .on_hover(move |inside| sink.set(inside));
    lay_out(&faded);

    faded.on_event(&moved(200.0, 200.0));
    assert!(hovered.get() && focus::text_entry_focused());

    level.set(0.0);
    assert!(
        !focus::text_entry_focused(),
        "a field nobody can see does not keep the keyboard, or it blocks every shortcut"
    );
    faded.on_event(&moved(200.0, 200.0));
    assert!(!hovered.get(), "and the hover it was showing goes with it");
}

#[test]
fn a_transparent_box_cuts_its_rect_out_of_the_opaque_box_around_it() {
    reset_layout_runtime();
    let (_, on_tap) = counter();
    let button = boxed(placed(0.0, 0.0, 20.0), vec![]).on_press(on_tap);
    let hole = boxed(placed(50.0, 50.0, 100.0), vec![Box::new(button)]).input_transparent();
    let panel = boxed(placed(0.0, 0.0, 200.0), vec![Box::new(hole)]).input_opaque();
    let root = Container::new(full(), vec![Box::new(panel)]).unwrap();
    lay_out(&root);

    assert!(
        claimed(10.0, 10.0),
        "the panel claims the pointer around the hole"
    );
    assert!(claimed(190.0, 120.0), "on every side of it");
    assert!(!claimed(100.0, 100.0), "but not inside it");
    assert!(
        claimed(60.0, 60.0),
        "except where something inside the hole answers the pointer"
    );
}

#[test]
fn the_pointer_passes_through_a_transparent_box_to_what_its_opaque_parent_covers() {
    reset_layout_runtime();
    let (pane_taps, pane_tap) = counter();
    let (button_taps, button_tap) = counter();
    let at: Rc<Cell<Option<(f32, f32)>>> = Rc::new(Cell::new(None));
    let sink = at.clone();
    let pane = boxed(full(), vec![])
        .on_press(pane_tap)
        .on_pointer_move(move |x, y| sink.set(Some((x, y))));
    let button = boxed(placed(0.0, 0.0, 20.0), vec![]).on_press(button_tap);
    let hole = boxed(placed(50.0, 50.0, 100.0), vec![Box::new(button)]).input_transparent();
    let panel = boxed(placed(0.0, 0.0, 200.0), vec![Box::new(hole)]).input_opaque();
    let mut root = Container::new(full(), vec![Box::new(pane), Box::new(panel)]).unwrap();
    lay_out(&root);

    tap(&mut root, 10.0, 10.0);
    root.on_event(&moved(10.0, 10.0));
    assert_eq!(
        (pane_taps.get(), at.get()),
        (0, None),
        "the panel covers the pane"
    );

    tap(&mut root, 100.0, 100.0);
    root.on_event(&moved(100.0, 100.0));
    assert_eq!(
        (pane_taps.get(), at.get()),
        (1, Some((100.0, 100.0))),
        "the hole in it does not"
    );

    tap(&mut root, 60.0, 60.0);
    assert_eq!(
        (button_taps.get(), pane_taps.get()),
        (1, 1),
        "a button inside the hole still takes its own press"
    );
}

#[test]
fn a_transparent_box_does_not_pierce_the_plain_box_around_it() {
    reset_layout_runtime();
    let hole = boxed(placed(50.0, 50.0, 100.0), vec![]).input_transparent();
    let plain = Container::new(placed(0.0, 0.0, 200.0), vec![Box::new(hole)]).unwrap();
    let panel = boxed(placed(0.0, 0.0, 200.0), vec![Box::new(plain)]).input_opaque();
    let root = Container::new(full(), vec![Box::new(panel)]).unwrap();
    lay_out(&root);
    assert!(
        claimed(100.0, 100.0),
        "a plain box between them stops the hole, so the panel keeps its whole rect"
    );
    drop(root);

    reset_layout_runtime();
    let (taps, on_tap) = counter();
    let at: Rc<Cell<Option<(f32, f32)>>> = Rc::new(Cell::new(None));
    let sink = at.clone();
    let pane = boxed(full(), vec![])
        .on_press(on_tap)
        .on_pointer_move(move |x, y| sink.set(Some((x, y))));
    let hole = boxed(placed(50.0, 50.0, 100.0), vec![]).input_transparent();
    let plain = Container::new(placed(0.0, 0.0, 200.0), vec![Box::new(hole)]).unwrap();
    let panel = boxed(placed(0.0, 0.0, 200.0), vec![Box::new(plain)]).input_opaque();
    let covering_hole = boxed(placed(50.0, 50.0, 100.0), vec![]).input_transparent();
    let cover = Container::new(
        rect_at(200.0, 0.0, 200.0, 200.0),
        vec![Box::new(covering_hole)],
    )
    .unwrap();
    let mut root = Container::new(
        full(),
        vec![Box::new(pane), Box::new(panel), Box::new(cover)],
    )
    .unwrap();
    lay_out(&root);

    tap(&mut root, 100.0, 100.0);
    root.on_event(&moved(100.0, 100.0));
    assert_eq!(
        (taps.get(), at.get()),
        (0, None),
        "inside an opaque panel, the plain box keeps the pane covered"
    );
    tap(&mut root, 300.0, 100.0);
    root.on_event(&moved(300.0, 100.0));
    assert_eq!(
        (taps.get(), at.get()),
        (0, None),
        "and a plain box alone covers what it is drawn over, hole or not"
    );
}

fn listening(log: &Rc<RefCell<Vec<String>>>, name: &'static str) -> StyledContainer {
    let heard = |what: &'static str| {
        let log = log.clone();
        move || log.borrow_mut().push(format!("{name} {what}"))
    };
    let (hovered, scrolled, keyed, focused) = (
        heard("hover"),
        heard("scroll"),
        heard("key"),
        heard("focus"),
    );
    boxed(LayoutStyle::new().width(100.0).height(100.0), vec![])
        .on_press(heard("press"))
        .on_hover(move |inside| {
            if inside {
                hovered();
            }
        })
        .on_scroll(move |_dx, _dy| scrolled())
        .on_key(move |_key| keyed())
        .on_focus(move |gained| {
            if gained {
                focused();
            }
        })
}

#[test]
fn an_inert_subtree_takes_no_pointer_no_keys_and_no_focus() {
    reset_layout_runtime();
    focus::clear();
    let log = Rc::new(RefCell::new(Vec::new()));
    let live = listening(&log, "live");
    let inner = listening(&log, "inner");
    let inner_node = inner.layout_node();
    let frozen = boxed(
        LayoutStyle::new().width(100.0).height(100.0),
        vec![Box::new(inner)],
    )
    .inert(|| true);
    let frozen_node = frozen.layout_node();
    let mut root = Container::new(
        LayoutStyle::new().flex_row().width(400.0).height(400.0),
        vec![Box::new(live), Box::new(frozen)],
    )
    .unwrap();
    lay_out(&root);

    for x in [50.0, 150.0] {
        root.on_event(&moved(x, 50.0));
        tap(&mut root, x, 50.0);
        root.on_event(&Event::Scrolled {
            delta: ScrollDelta::Lines { x: 0.0, y: -1.0 },
            x,
            y: 50.0,
        });
    }
    root.on_event(&Event::KeyPressed {
        key: Key::Char('a'),
        modifiers: ModifiersState::default(),
    });
    focus::clear();
    focus::focus_next();
    focus::focus_next();

    let heard = log.borrow().clone();
    for what in ["hover", "press", "scroll", "key", "focus"] {
        assert!(
            heard.contains(&format!("live {what}")),
            "the live sibling hears the {what}: {heard:?}"
        );
    }
    assert!(
        !heard.iter().any(|entry| entry.starts_with("inner")),
        "the inert subtree hears nothing: {heard:?}"
    );
    assert!(claimed(50.0, 50.0), "the live sibling claims its rect");
    assert!(!claimed(150.0, 50.0), "the inert subtree claims none");
    assert!(
        !focus::focus_first_in(frozen_node),
        "focus cannot be sent into it"
    );
    let id = focus::next_id();
    focus::register_at(id, FocusKind::Widget, inner_node);
    focus::request(id);
    assert!(
        !focus::is_focused(id),
        "nor requested for something inside it"
    );
}

/// The keyboard twin of an invisible box catching a click: a key carries no position to miss a hidden box with, so only the chain can keep an unseen shortcut table from firing.
#[test]
fn a_shortcut_table_in_a_hidden_subtree_hears_no_keys() {
    reset_layout_runtime();
    focus::clear();
    let (hidden_keys, hidden_key) = counter();
    let (live_keys, live_key) = counter();
    let table = boxed(placed(0.0, 0.0, 100.0), vec![]).on_key(move |_key| hidden_key());
    let pane = boxed(pane_style(), vec![Box::new(table)]);
    let pane_node = pane.layout_node();
    let live = boxed(placed(200.0, 0.0, 100.0), vec![]).on_key(move |_key| live_key());
    let mut root = Container::new(full(), vec![Box::new(pane), Box::new(live)]).unwrap();
    lay_out(&root);
    let key = Event::KeyPressed {
        key: Key::Char('a'),
        modifiers: ModifiersState::default(),
    };

    root.on_event(&key);
    assert_eq!(
        (hidden_keys.get(), live_keys.get()),
        (1, 1),
        "both tables hear the key while both panes are shown"
    );

    set_display(pane_node, false);
    mark_dirty(pane_node).unwrap();
    lay_out(&root);
    root.on_event(&key);
    assert_eq!(
        hidden_keys.get(),
        1,
        "a table under `display:none` hears nothing"
    );
    assert_eq!(
        live_keys.get(),
        2,
        "while the pane still shown keeps its keys"
    );
}

/// And the chain that has to be climbed is the registry's, not the layout tree's.
///
/// A scroll area lays its content out as a root of its own, with no layout link back to the viewport, so `display:none` above the scroll is invisible to anything walking parents. The registry keeps its own link across that gap, which is why the question is asked of it.
#[test]
fn a_shortcut_table_in_a_hidden_scroll_area_hears_no_keys() {
    reset_layout_runtime();
    focus::clear();
    let (keys, on_key) = counter();
    let mut table_node = None;
    let scroll = LayoutScrollArea::new_with(LayoutStyle::new().width(300.0).height(200.0), |_| {
        let table = boxed(LayoutStyle::new().width(300.0).height(100.0), vec![])
            .on_key(move |_key| on_key());
        table_node = Some(table.layout_node());
        Ok(Box::new(table) as Box<dyn LayoutItem>)
    })
    .unwrap();
    let pane = boxed(pane_style(), vec![Box::new(scroll)]);
    let pane_node = pane.layout_node();
    let mut root = Container::new(full(), vec![Box::new(pane)]).unwrap();
    lay_out(&root);
    let key = Event::KeyPressed {
        key: Key::Char('a'),
        modifiers: ModifiersState::default(),
    };

    root.on_event(&key);
    assert_eq!(keys.get(), 1, "shown, the table hears the key");

    set_display(pane_node, false);
    mark_dirty(pane_node).unwrap();
    lay_out(&root);
    assert!(
        !layout_reactive::is_hidden(table_node.unwrap()),
        "the layout links stop at the scroll's own root, so they never reach the hidden pane"
    );
    root.on_event(&key);
    assert_eq!(keys.get(), 1, "and the table hears nothing all the same");
}

#[test]
fn toggling_inert_adds_and_removes_the_rect_and_the_dispatch_at_once() {
    reset_layout_runtime();
    let locked = signal(false);
    let (taps, on_tap) = counter();
    let button = boxed(placed(0.0, 0.0, 100.0), vec![]).on_press(on_tap);
    let pane = boxed(full(), vec![Box::new(button)]).inert(move || locked.get());
    let mut root = Container::new(full(), vec![Box::new(pane)]).unwrap();
    lay_out(&root);

    assert!(claimed(50.0, 50.0));
    tap(&mut root, 50.0, 50.0);
    assert_eq!(taps.get(), 1);

    locked.set(true);
    assert!(!claimed(50.0, 50.0), "inert, with no relayout in between");
    tap(&mut root, 50.0, 50.0);
    assert_eq!(taps.get(), 1);

    locked.set(false);
    assert!(claimed(50.0, 50.0), "and live again");
    tap(&mut root, 50.0, 50.0);
    assert_eq!(taps.get(), 2);
}

#[test]
fn making_a_subtree_inert_takes_the_focus_out_of_it() {
    reset_layout_runtime();
    let field = boxed(LayoutStyle::new().width(50.0).height(20.0), vec![]);
    let id = focus::next_id();
    focus::register_at(id, FocusKind::Widget, field.layout_node());
    let wrapper = boxed(
        LayoutStyle::new().width(100.0).height(100.0),
        vec![Box::new(field)],
    );
    focus::request(id);
    assert!(focus::is_focused(id));

    let _wrapper = wrapper.inert(|| true);
    assert!(
        !focus::is_focused(id),
        "focus does not stay behind in a subtree that takes no keys"
    );
}

#[test]
fn an_overlay_opened_from_an_inert_subtree_receives_nothing_claims_nothing_and_takes_no_focus() {
    reset_layout_runtime();
    focus::clear();
    let host = new_container(full(), &[]).unwrap();
    compute_layout(
        host,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    set_overlay_host(host);

    let locked = signal(true);
    let (taps, on_tap) = counter();
    let button = boxed(LayoutStyle::new().width(80.0).height(40.0), vec![])
        .control(focus::Role::Button)
        .on_press(on_tap);
    let overlay = Overlay::new(LayoutStyle::new(), vec![Box::new(button)]).unwrap();
    let content = overlay.content_node();
    let pane = boxed(full(), vec![Box::new(overlay)]).inert(move || locked.get());
    let root = Container::new(full(), vec![Box::new(pane)]).unwrap();
    lay_out(&root);
    compute_layout(
        host,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();

    assert_eq!(tap_overlays(40.0, 20.0), EventResult::Ignored);
    assert_eq!(
        taps.get(),
        0,
        "the dropdown of an inert pane takes no press"
    );
    assert!(!claimed(40.0, 20.0), "and claims no region");
    assert!(!focus::focus_first_in(content), "and cannot take focus");

    locked.set(false);
    tap_overlays(40.0, 20.0);
    assert_eq!(taps.get(), 1, "live again, it answers");
    assert!(claimed(40.0, 20.0));
    assert!(focus::focus_first_in(content));

    locked.set(true);
    assert!(
        focus::current().is_none(),
        "and the focus it took is let go the moment its owner turns inert"
    );
}

#[test]
fn a_closed_overlay_claims_nothing_for_the_content_it_keeps_mounted() {
    reset_layout_runtime();
    let open = signal(false);
    let (_, on_tap) = counter();
    let button = boxed(LayoutStyle::new().width(80.0).height(40.0), vec![]).on_press(on_tap);
    let overlay = Overlay::toggleable(full(), vec![Box::new(button)], move || open.get()).unwrap();
    lay_out(&overlay);

    assert!(
        !claimed(40.0, 20.0),
        "a closed dialog's button claims nothing"
    );
    open.set(true);
    assert!(claimed(40.0, 20.0), "an open one claims it");
}

#[test]
fn a_text_field_and_a_scroll_area_claim_where_they_are_drawn() {
    reset_layout_runtime();
    let field = Input::new(
        signal(String::new()),
        LayoutStyle::new().width(200.0).height(30.0),
        || TextStyle::new(14.0, Color::BLACK),
    )
    .unwrap();
    let body = Canvas::new(LayoutStyle::new().width(300.0).height(600.0), |_| {
        RenderNode::Empty
    })
    .unwrap();
    let scroll = LayoutScrollArea::new(
        LayoutStyle::new().width(300.0).height(200.0),
        Box::new(body),
    )
    .unwrap();
    let root = Container::new(
        LayoutStyle::new().flex_column().width(400.0).height(400.0),
        vec![Box::new(field), Box::new(scroll)],
    )
    .unwrap();
    lay_out(&root);

    let region = interactive_rects();
    assert!(
        region.contains(&Rect::new(0.0, 0.0, 200.0, 30.0)),
        "{region:?}"
    );
    assert!(
        region.contains(&Rect::new(0.0, 30.0, 300.0, 200.0)),
        "{region:?}"
    );
    assert!(!claimed(350.0, 300.0));
}

fn scrolled_list(
    offset: (reactive_core::RwSignal<f32>, reactive_core::RwSignal<f32>),
    rows: Vec<Box<dyn LayoutItem>>,
) -> LayoutScrollArea {
    LayoutScrollArea::new_keeping(
        LayoutStyle::new().width(300.0).height(200.0),
        offset,
        move |_| {
            Ok(
                Box::new(Container::new(LayoutStyle::new().flex_column(), rows)?)
                    as Box<dyn LayoutItem>,
            )
        },
    )
    .unwrap()
}

#[test]
fn a_box_scrolled_half_out_of_its_viewport_claims_only_its_visible_half() {
    reset_layout_runtime();
    let offset = (signal(0.0f32), signal(0.0f32));
    let (taps, on_tap) = counter();
    let spacer = Container::new(LayoutStyle::new().width(300.0).height(250.0), vec![]).unwrap();
    let button = boxed(LayoutStyle::new().width(300.0).height(100.0), vec![]).on_press(on_tap);
    let button_node = button.layout_node();
    let mut scroll = scrolled_list(offset, vec![Box::new(spacer), Box::new(button)]);
    compute_layout(
        scroll.layout_node(),
        AvailableSpace::Definite(300.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();

    assert_eq!(
        pointable_rect(button_node),
        None,
        "below the fold it is nowhere"
    );
    offset.1.set(100.0);
    assert_eq!(
        pointable_rect(button_node),
        Some(Rect::new(0.0, 150.0, 300.0, 50.0)),
        "scrolled half in, only the half that is drawn can be pointed at"
    );

    tap(&mut scroll, 150.0, 175.0);
    assert_eq!(taps.get(), 1, "and that half takes the press");
    tap(&mut scroll, 150.0, 225.0);
    assert_eq!(taps.get(), 1, "the half the viewport cuts off does not");
}

#[test]
fn an_opaque_card_list_in_a_scroll_area_claims_where_the_cards_are_drawn() {
    reset_layout_runtime();
    let offset = (signal(0.0f32), signal(0.0f32));
    let cards: Vec<Box<dyn LayoutItem>> = (0..3)
        .map(|_| {
            Box::new(boxed(LayoutStyle::new().width(300.0).height(80.0), vec![]).input_opaque())
                as Box<dyn LayoutItem>
        })
        .collect();
    let first = cards[0].layout_node();
    let scroll = scrolled_list(offset, cards);
    let spacer = Container::new(LayoutStyle::new().width(400.0).height(120.0), vec![]).unwrap();
    let root = Container::new(
        LayoutStyle::new().flex_column().width(400.0).height(400.0),
        vec![Box::new(spacer), Box::new(scroll)],
    )
    .unwrap();
    lay_out(&root);

    assert_eq!(
        pointable_rect(first),
        Some(Rect::new(0.0, 120.0, 300.0, 80.0)),
        "the first card is drawn at the viewport's top, not where the content laid it out"
    );
    offset.1.set(40.0);
    assert_eq!(
        pointable_rect(first),
        Some(Rect::new(0.0, 120.0, 300.0, 40.0))
    );
    assert!(
        !claimed(150.0, 40.0),
        "nothing is claimed at the content-local position"
    );
    assert!(claimed(150.0, 200.0));
}

#[test]
fn a_transformed_box_claims_and_takes_the_pointer_where_it_is_drawn() {
    reset_layout_runtime();
    let (taps, on_tap) = counter();
    let moved_box = boxed(placed(0.0, 0.0, 100.0), vec![])
        .with_transform(|_r| Some([1.0, 0.0, 0.0, 1.0, 200.0, 0.0]))
        .on_press(on_tap);
    let mut root = Container::new(full(), vec![Box::new(moved_box)]).unwrap();
    lay_out(&root);

    assert_eq!(
        interactive_rects(),
        vec![Rect::new(200.0, 0.0, 100.0, 100.0)]
    );
    tap(&mut root, 50.0, 50.0);
    assert_eq!(taps.get(), 0, "where it was laid out, nothing is there");
    tap(&mut root, 250.0, 50.0);
    assert_eq!(taps.get(), 1, "where it is drawn, it answers");
}

/// A box turned 45° draws a diamond, and the bounds around a diamond are half empty corner.
///
/// Claiming those corners is a click the surface takes and then does nothing with, at a place where nothing is drawn. The claim is the drawn shape instead, stated as the pixel rows a region comes in, and the hit test asks the same shape.
#[test]
fn a_rotated_box_claims_the_shape_it_draws_and_not_the_corners_around_it() {
    reset_layout_runtime();
    let (_, on_tap) = counter();
    let turned = boxed(placed(100.0, 100.0, 100.0), vec![])
        .with_transform(|r| box_transform(r, 45.0, 1.0, 1.0, 0.0, 0.0))
        .on_press(on_tap);
    let root = Container::new(full(), vec![Box::new(turned)]).unwrap();
    lay_out(&root);

    assert!(claimed(150.0, 150.0), "the diamond claims its middle");
    assert!(claimed(150.0, 85.0), "out to the point of its top corner");
    assert!(
        !claimed(85.0, 85.0),
        "and nothing of the corner the bounds around it leave empty: {:?}",
        interactive_rects()
    );
    drop(root);

    reset_layout_runtime();
    let (pane_taps, pane_tap) = counter();
    let (card_taps, card_tap) = counter();
    let pane = boxed(full(), vec![]).on_press(pane_tap);
    let turned = boxed(placed(100.0, 100.0, 100.0), vec![])
        .with_transform(|r| box_transform(r, 45.0, 1.0, 1.0, 0.0, 0.0))
        .on_press(card_tap);
    let mut root = Container::new(full(), vec![Box::new(pane), Box::new(turned)]).unwrap();
    lay_out(&root);

    tap(&mut root, 150.0, 150.0);
    assert_eq!(
        (card_taps.get(), pane_taps.get()),
        (1, 0),
        "a press on the diamond is the diamond's"
    );
    tap(&mut root, 85.0, 85.0);
    assert_eq!(
        (card_taps.get(), pane_taps.get()),
        (1, 1),
        "one in the empty corner reaches the pane beneath it"
    );
}

#[test]
fn an_anchored_panel_claims_and_takes_the_pointer_where_it_is_drawn() {
    reset_layout_runtime();
    let (taps, on_tap) = counter();
    let button = boxed(LayoutStyle::new().width(80.0).height(40.0), vec![]).on_press(on_tap);
    let overlay = Overlay::anchored_click_through(
        LayoutStyle::new(),
        vec![Box::new(button)],
        signal(Rect::new(100.0, 100.0, 50.0, 20.0)),
        crate::overlay::Placement::Below,
    )
    .unwrap();
    lay_out(&overlay);

    assert!(claimed(140.0, 144.0), "{:?}", interactive_rects());
    assert!(!claimed(40.0, 20.0), "not at the origin it was laid out at");
    tap_overlays(140.0, 144.0);
    assert_eq!(taps.get(), 1);
}

#[test]
fn a_gate_is_withdrawn_with_its_box() {
    reset_layout_runtime();
    let frozen = boxed(full(), vec![]).inert(|| true);
    let frozen_node = frozen.layout_node();
    drop(frozen);
    let (_, on_tap) = counter();
    let button = boxed(placed(0.0, 0.0, 50.0), vec![]).on_press(on_tap);
    set_children(frozen_node, &[button.layout_node()]).unwrap();
    compute_layout(
        frozen_node,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    assert!(
        claimed(25.0, 25.0),
        "the node the gate stood on no longer shuts anything"
    );
}

#[test]
fn a_reset_forgets_what_a_recycled_node_id_would_inherit() {
    reset_layout_runtime();
    let stale = boxed(full(), vec![]).inert(|| true);
    let stale_node = stale.layout_node();
    reset_layout_runtime();
    let (_, on_tap) = counter();
    let fresh = boxed(placed(0.0, 0.0, 50.0), vec![]).on_press(on_tap);
    assert_eq!(
        fresh.layout_node(),
        stale_node,
        "the ids must really collide"
    );
    lay_out(&fresh);
    assert!(
        claimed(25.0, 25.0),
        "the stale gate does not shut the fresh box"
    );
    drop(stale);
    assert!(
        claimed(25.0, 25.0),
        "and the stale box's withdrawal does not take the fresh box's claim with it"
    );
}

#[test]
fn two_surfaces_whose_ids_collide_do_not_steer_each_others_input() {
    let a = Surface::new();
    let b = Surface::new();

    let (scroll_in_a, content_in_a) = {
        let _entered = a.enter();
        let offset = (signal(0.0f32), signal(100.0f32));
        let mut content = None;
        let scroll = LayoutScrollArea::new_keeping(
            LayoutStyle::new().width(300.0).height(200.0),
            offset,
            |_| {
                let body = boxed(LayoutStyle::new().width(300.0).height(600.0), vec![]);
                content = Some(body.layout_node());
                Ok(Box::new(body) as Box<dyn LayoutItem>)
            },
        )
        .unwrap();
        (scroll, content.unwrap())
    };

    {
        let _entered = b.enter();
        let inner = boxed(placed(0.0, 0.0, 50.0), vec![]).input_transparent();
        let middle = boxed(placed(50.0, 50.0, 100.0), vec![Box::new(inner)]).input_transparent();
        assert_eq!(
            middle.layout_node(),
            content_in_a,
            "the two surfaces must really collide on ids, or this proves nothing"
        );
        let panel = boxed(placed(0.0, 0.0, 200.0), vec![Box::new(middle)]).input_opaque();
        let root = Container::new(full(), vec![Box::new(panel)]).unwrap();
        lay_out(&root);
        assert!(claimed(10.0, 10.0), "b's panel is where b laid it out");
        assert!(
            !claimed(100.0, 100.0),
            "and a's scroll content, under the same id, does not stop b's hole"
        );
    }

    let _entered = a.enter();
    drop(scroll_in_a);
}

#[test]
fn the_region_merges_what_the_holes_cut_apart() {
    assert_eq!(
        merge(vec![
            Rect::new(0.0, 0.0, 10.0, 10.0),
            Rect::new(0.0, 10.0, 10.0, 10.0),
            Rect::new(2.0, 2.0, 3.0, 3.0),
        ]),
        vec![Rect::new(0.0, 0.0, 10.0, 20.0)]
    );
}

#[test]
fn a_box_whose_transform_follows_its_own_drag_is_measured_from_its_press() {
    reset_layout_runtime();
    let offset = signal(0.0f32);
    let reported = Rc::new(Cell::new((0.0f32, 0.0f32)));
    let sink = reported.clone();
    let carried = boxed(placed(0.0, 0.0, 100.0), vec![])
        .with_transform(move |_r| Some([1.0, 0.0, 0.0, 1.0, offset.get(), 0.0]))
        .on_drag(move |x, y| {
            sink.set((x, y));
            offset.set(x - 50.0);
        });
    let mut root = Container::new(full(), vec![Box::new(carried)]).unwrap();
    lay_out(&root);

    root.on_event(&press_with(50.0, 50.0, PointerButton::Primary));
    root.on_event(&moved(150.0, 50.0));
    root.on_event(&moved(250.0, 50.0));
    assert_eq!(
        reported.get(),
        (250.0, 50.0),
        "the drag reads the pointer in the frame it was pressed in, not the one it is carried to"
    );
    root.on_event(&release_with(250.0, 50.0, PointerButton::Primary));
    assert!(
        claimed(250.0, 50.0) && !claimed(50.0, 50.0),
        "and it claims where it was left"
    );
}
