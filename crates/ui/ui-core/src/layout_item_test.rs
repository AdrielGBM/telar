use super::*;
use crate::StyledContainer;
use crate::context::{compute_layout, reset_layout_runtime};
use layout_core::AvailableSpace;
use platform_core::{PointerButton, PointerSource};
use renderer_core::RectStyle;
use std::cell::Cell;
use std::rc::Rc;

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

/// A press outside the clip does not reach the subtree, and one inside it still does.
///
/// The case this exists for: a row of items wider than the box it is clipped to. The overflow is not drawn, so whatever is painted over that strip looks like the only thing there — and a hidden item that still answered a click there would be stealing it from the visible one.
#[test]
fn a_press_outside_the_clip_never_reaches_what_it_hides() {
    let pressed = Rc::new(Cell::new(false));
    let sink = Rc::clone(&pressed);
    reset_layout_runtime();
    let inner = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .width(100.0)
            .height(20.0)
            .flex_shrink(0.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .on_press(move || sink.set(true));
    let mut clipped = ClippedItem::new(
        Box::new(
            StyledContainer::new(
                LayoutStyle::new().flex_row().width(40.0).height(20.0),
                |_r| RectStyle::default(),
                vec![Box::new(inner)],
            )
            .unwrap(),
        ),
        Clip::both(),
    );
    compute_layout(
        clipped.layout_node(),
        AvailableSpace::Definite(40.0),
        AvailableSpace::Definite(20.0),
    )
    .unwrap();

    clipped.on_event(&press(80.0, 10.0));
    clipped.on_event(&release(80.0, 10.0));
    assert!(
        !pressed.get(),
        "a tap 40px past the clip's edge reached the item hidden behind it"
    );

    clipped.on_event(&press(10.0, 10.0));
    clipped.on_event(&release(10.0, 10.0));
    assert!(
        pressed.get(),
        "and a tap on the part that is actually drawn still has to land"
    );
}

/// A clip is a shape, not an axis: the renderer's clip node has always taken a radius, and an inset is what a stroked box wants so the cut sits inside the border rather than under it.
#[test]
fn a_rounded_inset_clip_cuts_the_shape_it_names() {
    reset_layout_runtime();
    let clipped = ClippedItem::new(
        Box::new(
            StyledContainer::new(
                LayoutStyle::new().flex_row().width(40.0).height(20.0),
                |_r| RectStyle::default(),
                vec![],
            )
            .unwrap(),
        ),
        Clip::both().rounded(6.0).inset(2.0),
    );
    compute_layout(
        clipped.layout_node(),
        AvailableSpace::Definite(40.0),
        AvailableSpace::Definite(20.0),
    )
    .unwrap();

    let RenderNode::Clip { rect, radius, .. } = clipped.view() else {
        panic!("a clipped item renders a clip node");
    };
    assert_eq!(rect, Rect::new(2.0, 2.0, 36.0, 16.0));
    assert_eq!(radius, BorderRadius::all(6.0));
}

/// A frame drawn over a canvas cuts the paint and nothing else: a node dragged out past the edge has to keep following the hand, and a clip that swallowed the moves would drop it at the border. This is what a project had a hand-written widget for, beside the one that stops the pointer.
#[test]
fn a_paint_only_clip_lets_the_pointer_through() {
    let pressed = Rc::new(Cell::new(false));
    let sink = Rc::clone(&pressed);
    reset_layout_runtime();
    let inner = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .width(100.0)
            .height(20.0)
            .flex_shrink(0.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .on_press(move || sink.set(true));
    let mut clipped = ClippedItem::new(
        Box::new(
            StyledContainer::new(
                LayoutStyle::new().flex_row().width(40.0).height(20.0),
                |_r| RectStyle::default(),
                vec![Box::new(inner)],
            )
            .unwrap(),
        ),
        Clip::both().paint_only(),
    );
    compute_layout(
        clipped.layout_node(),
        AvailableSpace::Definite(40.0),
        AvailableSpace::Definite(20.0),
    )
    .unwrap();

    clipped.on_event(&press(80.0, 10.0));
    clipped.on_event(&release(80.0, 10.0));
    assert!(
        pressed.get(),
        "a press past the cut edge still reaches what it is over"
    );
}

/// One axis cut, the other left free — the thing CSS cannot say. The inset applies to the cutting edges only, since the free axis has no edge to pull in from.
#[test]
fn a_one_way_clip_leaves_the_other_axis_unbounded() {
    reset_layout_runtime();
    let clipped = ClippedItem::new(
        Box::new(
            StyledContainer::new(
                LayoutStyle::new().flex_row().width(40.0).height(20.0),
                |_r| RectStyle::default(),
                vec![],
            )
            .unwrap(),
        ),
        Clip::x().inset(2.0),
    );
    compute_layout(
        clipped.layout_node(),
        AvailableSpace::Definite(40.0),
        AvailableSpace::Definite(20.0),
    )
    .unwrap();

    let RenderNode::Clip { rect, .. } = clipped.view() else {
        panic!("a clipped item renders a clip node");
    };
    assert_eq!(rect.x, 2.0);
    assert_eq!(rect.width, 36.0);
    assert!(
        rect.y < -1.0e5 && rect.height > 1.0e6,
        "clipping one axis must leave the other unbounded: {rect:?}"
    );
}
