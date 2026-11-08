use std::cell::Cell;
use std::time::Duration;

use layout_core::{AvailableSpace, LayoutError, LayoutStyle};
use motion_core::{Easing, set_scale, tick, tween};
use platform_core::{PointerButton, PointerSource};
use renderer_core::RectStyle;
use web_time::Instant;

use super::*;
use crate::container::Container;
use crate::context::{
    compute_layout, new_container, relayout_if_dirty, reset_layout_runtime, set_layout_style,
};
use crate::layout_item::box_item;
use crate::styled_container::StyledContainer;
use crate::surface::IDENTITY;

const SLIDE_MS: u64 = 200;

fn fresh() {
    reset_layout_runtime();
    motion_core::reset();
    set_scale(1.0);
}

fn button(presses: Rc<Cell<u32>>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    Ok(box_item(
        StyledContainer::new(
            LayoutStyle::new().width(10.0).height(10.0),
            |_| RectStyle::default(),
            vec![],
        )?
        .on_press(move || presses.set(presses.get() + 1)),
    ))
}

fn press(target: &mut dyn LayoutItem, x: f64, y: f64) -> EventResult {
    let (button, source) = (PointerButton::Primary, PointerSource::Mouse);
    let result = target.on_event(&Event::PointerPressed {
        x,
        y,
        button,
        source: source.clone(),
    });
    target.on_event(&Event::PointerReleased {
        x,
        y,
        button,
        source,
    });
    result
}

fn drawn_y(transition: &LayoutTransition) -> f32 {
    track_layout(transition.layout_node()).unwrap().peek().y + transition.child.matrix_now()[5]
}

#[test]
fn a_node_pushed_down_by_a_growing_sibling_slides_there_and_takes_the_pointer_on_the_way() {
    fresh();
    let presses = Rc::new(Cell::new(0));
    let spacer = Container::new(LayoutStyle::new().width(10.0).height(10.0), vec![]).unwrap();
    let mut moving = animate_layout(
        button(Rc::clone(&presses)).unwrap(),
        tween(Duration::from_millis(SLIDE_MS), Easing::Linear),
    );
    let column = new_container(
        LayoutStyle::new().flex_column(),
        &[spacer.layout_node(), moving.layout_node()],
    )
    .unwrap();
    compute_layout(
        column,
        AvailableSpace::Definite(100.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();
    assert_eq!(moving.child.matrix_now(), IDENTITY);
    assert_eq!(drawn_y(&moving), 10.0);

    set_layout_style(
        spacer.layout_node(),
        LayoutStyle::new().width(10.0).height(30.0),
    )
    .unwrap();
    relayout_if_dirty();
    assert_eq!(
        track_layout(moving.layout_node()).unwrap().peek().y,
        30.0,
        "laid out below the grown sibling"
    );
    assert_eq!(drawn_y(&moving), 10.0, "still drawn where it was");
    assert_eq!(press(&mut moving, 5.0, 15.0), EventResult::Handled);
    assert_eq!(presses.get(), 1, "pressed where it is drawn");

    let start = Instant::now();
    tick(start);
    tick(start + Duration::from_millis(SLIDE_MS / 2));
    assert!(
        (drawn_y(&moving) - 20.0).abs() < 1e-3,
        "{}",
        drawn_y(&moving)
    );
    assert_eq!(press(&mut moving, 5.0, 25.0), EventResult::Handled);
    assert_eq!(presses.get(), 2);

    tick(start + Duration::from_millis(SLIDE_MS));
    assert_eq!(drawn_y(&moving), 30.0);
    assert_eq!(press(&mut moving, 5.0, 15.0), EventResult::Ignored);
    assert_eq!(press(&mut moving, 5.0, 35.0), EventResult::Handled);
    assert_eq!(presses.get(), 3);
}

#[test]
fn a_node_moved_only_by_its_parent_is_left_to_the_parent() {
    fresh();
    let spacer = Container::new(LayoutStyle::new().width(10.0).height(10.0), vec![]).unwrap();
    let inner = animate_layout(
        Container::new(LayoutStyle::new().width(10.0).height(10.0), vec![]).unwrap(),
        tween(Duration::from_millis(SLIDE_MS), Easing::Linear),
    );
    let holder = new_container(LayoutStyle::new().flex_column(), &[inner.layout_node()]).unwrap();
    let column = new_container(
        LayoutStyle::new().flex_column(),
        &[spacer.layout_node(), holder],
    )
    .unwrap();
    compute_layout(
        column,
        AvailableSpace::Definite(100.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();

    set_layout_style(
        spacer.layout_node(),
        LayoutStyle::new().width(10.0).height(30.0),
    )
    .unwrap();
    relayout_if_dirty();
    assert_eq!(track_layout(inner.layout_node()).unwrap().peek().y, 30.0);
    assert_eq!(
        inner.child.matrix_now(),
        IDENTITY,
        "its place inside its parent did not change"
    );
}

#[test]
fn a_node_rejoining_a_parent_that_moved_while_it_was_out_slides_from_where_it_was_drawn() {
    fresh();
    let spacer = Container::new(LayoutStyle::new().width(10.0).height(10.0), vec![]).unwrap();
    let moving = animate_layout(
        Container::new(LayoutStyle::new().width(10.0).height(10.0), vec![]).unwrap(),
        tween(Duration::from_millis(SLIDE_MS), Easing::Linear),
    );
    let node = moving.layout_node();
    let holder = new_container(LayoutStyle::new().flex_column(), &[node]).unwrap();
    let column = new_container(
        LayoutStyle::new().flex_column(),
        &[spacer.layout_node(), holder],
    )
    .unwrap();
    compute_layout(
        column,
        AvailableSpace::Definite(100.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();
    assert_eq!(drawn_y(&moving), 10.0);

    moving.child.set_out_of_flow(true);
    crate::context::set_children(holder, &[]).unwrap();
    set_layout_style(
        spacer.layout_node(),
        LayoutStyle::new().width(10.0).height(30.0),
    )
    .unwrap();
    relayout_if_dirty();
    assert_eq!(drawn_y(&moving), 10.0, "frozen where it was while out");

    moving.child.set_out_of_flow(false);
    crate::context::set_children(holder, &[node]).unwrap();
    relayout_if_dirty();
    assert_eq!(track_layout(node).unwrap().peek().y, 30.0);
    assert_eq!(drawn_y(&moving), 10.0, "rejoined, still drawn where it was");

    let start = Instant::now();
    tick(start);
    tick(start + Duration::from_millis(SLIDE_MS));
    assert_eq!(drawn_y(&moving), 30.0, "and slid to its place");
}
