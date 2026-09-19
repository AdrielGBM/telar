use layout_core::{AvailableSpace, LayoutStyle};
use platform_core::{Cursor, Event, PointerButton, PointerSource, WindowCommand};
use reactive_core::signal;
use renderer_core::RectStyle;
use ui_tree::Component;

use super::requested_cursor;
use crate::context::{compute_layout, reset_layout_runtime};
use crate::layout_item::{LayoutItem, box_item};
use crate::styled_container::StyledContainer;

fn moved(x: f64, y: f64) -> Event {
    Event::PointerMoved {
        x,
        y,
        source: PointerSource::Mouse,
    }
}

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

/// A 200x100 card showing `Grab`, holding a 40x40 grip at its top-left showing `EwResize` that drags.
fn card_with_grip() -> StyledContainer {
    let grip = StyledContainer::new(
        LayoutStyle::new().width(40.0).height(40.0),
        |_| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .cursor(Cursor::EwResize)
    .on_drag(|_, _| {});
    let card = StyledContainer::new(
        LayoutStyle::new().flex_column().width(200.0).height(100.0),
        |_| RectStyle::default(),
        vec![box_item(grip)],
    )
    .unwrap()
    .cursor(Cursor::Grab);
    compute_layout(
        card.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();
    card
}

fn last_request() -> Option<Cursor> {
    platform_core::take_window_commands()
        .into_iter()
        .rev()
        .find_map(|command| match command {
            WindowCommand::SetCursor(cursor) => Some(cursor),
            _ => None,
        })
}

#[test]
fn the_innermost_hovered_box_decides_and_leaving_it_hands_the_shape_back() {
    reset_layout_runtime();
    let _ = platform_core::take_window_commands();
    let mut card = card_with_grip();

    card.on_event(&moved(20.0, 20.0));
    assert_eq!(
        last_request(),
        Some(Cursor::EwResize),
        "entering both at once"
    );

    card.on_event(&moved(120.0, 20.0));
    assert_eq!(
        last_request(),
        Some(Cursor::Grab),
        "back over the card only"
    );

    card.on_event(&moved(20.0, 20.0));
    assert_eq!(last_request(), Some(Cursor::EwResize));

    card.on_event(&moved(500.0, 500.0));
    assert_eq!(last_request(), Some(Cursor::Default), "out of everything");
    assert_eq!(requested_cursor(), Cursor::Default);
}

#[test]
fn a_drag_keeps_its_shape_wherever_the_pointer_goes_until_it_ends() {
    reset_layout_runtime();
    let _ = platform_core::take_window_commands();
    let mut card = card_with_grip();

    card.on_event(&moved(20.0, 20.0));
    card.on_event(&press(20.0, 20.0));
    card.on_event(&moved(120.0, 20.0));
    assert_eq!(
        requested_cursor(),
        Cursor::EwResize,
        "over the card, still dragging the grip"
    );
    card.on_event(&moved(500.0, 500.0));
    assert_eq!(
        requested_cursor(),
        Cursor::EwResize,
        "outside everything, still dragging"
    );

    card.on_event(&release(500.0, 500.0));
    assert_eq!(
        requested_cursor(),
        Cursor::Default,
        "the drag ended outside"
    );
}

#[test]
fn a_shape_that_reads_a_signal_follows_it_while_hovered() {
    reset_layout_runtime();
    let _ = platform_core::take_window_commands();
    let shape = signal(Cursor::Grab);
    let mut card = StyledContainer::new(
        LayoutStyle::new().width(50.0).height(50.0),
        |_| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .cursor(reactive_core::Reactive::of(move || shape.get()));
    compute_layout(
        card.layout_node(),
        AvailableSpace::Definite(50.0),
        AvailableSpace::Definite(50.0),
    )
    .unwrap();

    card.on_event(&moved(10.0, 10.0));
    assert_eq!(requested_cursor(), Cursor::Grab);
    shape.set(Cursor::Grabbing);
    assert_eq!(requested_cursor(), Cursor::Grabbing);

    drop(card);
    assert_eq!(
        requested_cursor(),
        Cursor::Default,
        "a box that goes gives its shape back"
    );
}

#[test]
fn a_tree_dropped_after_a_reset_leaves_the_new_trees_claims_alone() {
    reset_layout_runtime();
    let old = card_with_grip();
    reset_layout_runtime();
    let mut card = card_with_grip();
    drop(old);
    card.on_event(&moved(20.0, 20.0));
    assert_eq!(requested_cursor(), Cursor::EwResize);
}

#[test]
fn a_claim_dropped_under_another_surface_is_withdrawn_from_its_own() {
    use crate::surface_context::Surface;

    let (first, second) = (Surface::new(), Surface::new());
    let mut card = {
        let _in_first = first.enter();
        reset_layout_runtime();
        let mut card = card_with_grip();
        card.on_event(&moved(120.0, 20.0));
        assert_eq!(requested_cursor(), Cursor::Grab);
        card
    };
    {
        let _in_second = second.enter();
        let mut other = card_with_grip();
        other.on_event(&moved(20.0, 20.0));
        assert_eq!(requested_cursor(), Cursor::EwResize);
        drop(std::mem::replace(&mut card, card_with_grip()));
        assert_eq!(
            requested_cursor(),
            Cursor::EwResize,
            "the second window keeps its own shape"
        );
        drop(card);
        drop(other);
    }
    let _in_first = first.enter();
    assert_eq!(
        requested_cursor(),
        Cursor::Default,
        "the first window's shape went with its box"
    );
}

#[test]
fn a_claim_that_outlives_its_surface_leaves_other_surfaces_alone() {
    use super::CursorClaim;

    reset_layout_runtime();
    let surface = crate::Surface::new();
    let orphan = {
        let _entered = surface.enter();
        CursorClaim::new(Cursor::Grab)
    };
    drop(surface);
    let here = CursorClaim::new(Cursor::Text);
    here.hover(1, true);
    let _ = platform_core::take_window_commands();

    drop(orphan);

    assert_eq!(requested_cursor(), Cursor::Text);
    assert_eq!(
        last_request(),
        None,
        "and nothing is pushed to this surface"
    );
}

#[test]
fn a_panicking_dispatch_gives_its_depth_back() {
    use super::{depth, nested};

    let before = depth();
    let outcome = std::panic::catch_unwind(|| nested(|| panic!("mid-dispatch")));
    assert!(outcome.is_err());
    assert_eq!(depth(), before);
}
