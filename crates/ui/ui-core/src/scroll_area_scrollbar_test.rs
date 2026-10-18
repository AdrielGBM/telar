use super::tests::{make_scroll_area, make_scroll_area_wide};
use super::*;
use crate::canvas::Canvas;
use crate::context::{compute_layout, reset_layout_runtime};
use layout_core::AvailableSpace;
use platform_core::{PointerButton, PointerSource};

pub(super) fn press(x: f32, y: f32) -> Event {
    Event::PointerPressed {
        x: x as f64,
        y: y as f64,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    }
}

pub(super) fn moved(x: f32, y: f32) -> Event {
    Event::PointerMoved {
        x: x as f64,
        y: y as f64,
        source: PointerSource::Mouse,
    }
}

fn released(x: f32, y: f32) -> Event {
    Event::PointerReleased {
        x: x as f64,
        y: y as f64,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    }
}

// Viewport 400x300 over content 400x1000: a 90px thumb with 210px of travel over 700px of scroll.
fn tall() -> ScrollArea {
    make_scroll_area()
}

/// The content moves by its own range, not by the distance the thumb was dragged.
#[test]
fn dragging_the_thumb_moves_the_content_in_proportion() {
    let mut sa = tall();
    sa.on_event(&moved(396.0, 45.0));
    assert_eq!(sa.on_event(&press(396.0, 45.0)), EventResult::Handled);
    sa.on_event(&moved(396.0, 150.0));
    assert_eq!(
        sa.core.scroll_y.get(),
        350.0,
        "half the travel, half the range"
    );
    sa.on_event(&moved(396.0, 400.0));
    assert_eq!(sa.core.scroll_y.get(), 700.0, "and it stops at the end");
}

#[test]
fn the_thumb_is_grabbed_where_it_was_taken_hold_of() {
    let mut sa = tall();
    // Held near its lower edge: the thumb must not jump so its top lands under the pointer.
    sa.on_event(&moved(396.0, 80.0));
    sa.on_event(&press(396.0, 80.0));
    sa.on_event(&moved(396.0, 80.0));
    assert_eq!(sa.core.scroll_y.get(), 0.0, "no jump on the first move");
}

#[test]
fn a_press_on_the_track_pages_towards_it() {
    let mut sa = tall();
    sa.on_event(&moved(396.0, 200.0));
    sa.on_event(&press(396.0, 200.0));
    assert_eq!(sa.core.scroll_y.get(), 300.0, "one viewport down");
    sa.on_event(&released(396.0, 200.0));
    sa.on_event(&moved(396.0, 5.0));
    sa.on_event(&press(396.0, 5.0));
    assert_eq!(sa.core.scroll_y.get(), 0.0, "and back up again");
}

#[test]
fn letting_go_ends_the_drag() {
    let mut sa = tall();
    sa.on_event(&moved(396.0, 45.0));
    sa.on_event(&press(396.0, 45.0));
    sa.on_event(&moved(396.0, 150.0));
    sa.on_event(&released(396.0, 150.0));
    let settled = sa.core.scroll_y.get();
    sa.on_event(&moved(396.0, 250.0));
    assert_eq!(sa.core.scroll_y.get(), settled, "the pointer is free again");
}

/// The bar is a strip at the edge; a press anywhere else is the content's.
#[test]
fn a_press_beside_the_bar_is_not_a_grab() {
    let mut sa = tall();
    sa.on_event(&press(100.0, 45.0));
    sa.on_event(&moved(100.0, 250.0));
    assert_eq!(sa.core.scroll_y.get(), 0.0);
}

#[test]
fn there_is_nothing_to_grab_when_the_content_fits() {
    reset_layout_runtime();
    let content = Canvas::new(LayoutStyle::new().width(400.0).height(120.0), |_| {
        RenderNode::Empty
    })
    .unwrap();
    let node = content.layout_node();
    let mut sa = ScrollArea::new(|| Rect::new(0.0, 0.0, 400.0, 300.0), Box::new(content));
    compute_layout(
        node,
        AvailableSpace::Definite(400.0),
        AvailableSpace::MaxContent,
    )
    .unwrap();
    assert_ne!(
        sa.on_event(&press(396.0, 45.0)),
        EventResult::Handled,
        "no bar is drawn, so nothing there answers a press"
    );
}

/// Viewport 400x300 over content 1000x300: a 160px thumb with 240px of travel over 600px of scroll.
#[test]
fn the_horizontal_bar_drags_the_same_way() {
    let mut sa = make_scroll_area_wide();
    sa.on_event(&moved(80.0, 296.0));
    assert_eq!(sa.on_event(&press(80.0, 296.0)), EventResult::Handled);
    sa.on_event(&moved(200.0, 296.0));
    assert_eq!(sa.core.scroll_x.get(), 300.0);
}

/// On a surface drawn in whole cells a thumb is whole cells long and starts on one, wherever the scroll has taken it: a fractional thumb is drawn one cell longer at some offsets than at others.
#[test]
fn on_a_cell_grid_the_thumb_is_whole_cells_wherever_it_is() {
    let grid = geometry_core::LayoutGrid::new(8.0, 16.0);
    for scroll in [0.0, 5.0, 8.0, 13.0, 16.0] {
        let thumb = Thumb::of(Axis::Vertical, grid, 0.0, 896.0, 912.0, scroll).unwrap();
        assert_eq!(thumb.length % 16.0, 0.0, "{} long", thumb.length);
        assert_eq!(thumb.start % 16.0, 0.0, "at {}", thumb.start);
    }
    let thumb = Thumb::of(Axis::Horizontal, grid, 0.0, 300.0, 1000.0, 0.0).unwrap();
    assert_eq!(thumb.length % 8.0, 0.0, "{} wide", thumb.length);
    let unit = Thumb::of(
        Axis::Vertical,
        geometry_core::LayoutGrid::new(1.0, 1.0),
        0.0,
        300.0,
        1000.0,
        0.0,
    )
    .unwrap();
    assert_eq!(
        unit.length, 90.0,
        "a surface that does not quantise keeps the exact length"
    );
}

#[derive(Clone)]
struct Silent;
impl theme_core::ThemeTokens for Silent {}

#[derive(Clone)]
struct Chunky;
impl theme_core::ThemeTokens for Chunky {
    fn scrollbar_width(&self) -> f32 {
        12.0
    }
}

fn thumb_rect(view: RenderNode) -> (f32, BorderRadius) {
    let RenderNode::Group { children, .. } = view else {
        panic!("expected Group");
    };
    match &children[1] {
        RenderNode::Primitive(renderer_core::DrawCommand::Rect { rect, style }) => {
            (rect.width, style.radius)
        }
        _ => panic!("expected the thumb"),
    }
}

#[test]
fn default_style_follows_theme_and_control_size_after_construction() {
    theme_core::set_theme(Silent);
    theme_core::set_control_size(theme_core::ControlSize::Regular);
    let sa = tall();
    sa.core.bars_up();
    assert_eq!(thumb_rect(sa.view()), (6.0, BorderRadius::all(3.0)));

    theme_core::set_theme(Chunky);
    assert_eq!(thumb_rect(sa.view()), (10.0, BorderRadius::all(5.0)));

    theme_core::set_control_size(theme_core::ControlSize::Large);
    assert_eq!(thumb_rect(sa.view()), (13.0, BorderRadius::all(5.0)));

    theme_core::set_control_size(theme_core::ControlSize::Regular);
    theme_core::set_theme(Silent);
}

fn settle_fade() {
    let now = std::time::Instant::now();
    motion_core::tick(now);
    motion_core::tick(now + BARS_FADE * 2);
}

fn idle() {
    reactive_core::advance_timer_clock(BARS_IDLE);
    reactive_core::fire_timers();
    settle_fade();
}

fn bars_drawn(sa: &ScrollArea) -> bool {
    let RenderNode::Group { children, .. } = sa.view() else {
        panic!("expected Group");
    };
    !matches!(&children[1], RenderNode::Empty)
}

#[test]
fn the_bars_keep_out_of_the_way_until_the_content_moves_and_fade_once_it_rests() {
    reactive_core::reset_timers();
    let mut sa = tall();
    assert!(
        !bars_drawn(&sa),
        "at rest, nothing is drawn over the content"
    );

    sa.on_event(&Event::Scrolled {
        delta: ScrollDelta::Pixels { x: 0.0, y: -40.0 },
        x: 200.0,
        y: 150.0,
    });
    settle_fade();
    assert!(bars_drawn(&sa), "moving the content brings them up");

    idle();
    assert!(!bars_drawn(&sa), "and they fade once it has rested");
}

#[test]
fn the_bars_stay_up_while_the_pointer_is_on_their_lane() {
    reactive_core::reset_timers();
    let mut sa = tall();
    sa.on_event(&moved(396.0, 150.0));
    settle_fade();
    assert!(
        bars_drawn(&sa),
        "the lane brings them up, so a bar can be found"
    );
    idle();
    assert!(
        bars_drawn(&sa),
        "and keeps them up however long the pointer rests there"
    );

    sa.on_event(&moved(200.0, 150.0));
    idle();
    assert!(!bars_drawn(&sa), "leaving the lane lets them fade");
}

#[test]
fn a_held_bar_stays_up() {
    reactive_core::reset_timers();
    let mut sa = tall();
    sa.on_event(&moved(396.0, 45.0));
    sa.on_event(&press(396.0, 45.0));
    sa.on_event(&moved(200.0, 150.0));
    idle();
    assert!(
        bars_drawn(&sa),
        "dragged off its lane, the bar still in hand stays"
    );
    sa.on_event(&released(200.0, 150.0));
    idle();
    assert!(!bars_drawn(&sa));
}

/// A finger has no hover to bring a hidden bar up first, so a tap at the content's edge is the content's, not a page down it never saw coming.
#[test]
fn a_press_on_a_hidden_bar_s_lane_is_the_content_s() {
    reactive_core::reset_timers();
    let mut sa = tall();
    assert_ne!(sa.on_event(&press(396.0, 200.0)), EventResult::Handled);
    assert_eq!(sa.core.scroll_y.get(), 0.0, "nothing paged");
}
