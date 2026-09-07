use crate::context::reset_layout_runtime;
use std::cell::Cell;
use std::rc::Rc;

use layout_core::AvailableSpace;
use renderer_core::{Color, DrawCommand, Paint, RectStyle, ShapeStyle};

use super::*;
use crate::context::{compute_layout, new_container};
use crate::layout_item::LayoutItem;

// `draw` must be re-invoked on every `view()`: a `$signal` colour read inside it would otherwise freeze at whatever was current when the closure was built.
#[test]
fn draw_closure_is_re_read_each_view_and_recolors() {
    let color = Rc::new(Cell::new(Color::RED));
    let color_read = color.clone();
    reset_layout_runtime();
    let canvas = Canvas::new(LayoutStyle::new().width(40.0).height(40.0), move |r| {
        RenderNode::rect(r, RectStyle::default().with_fill(color_read.get()))
    })
    .unwrap();
    let root = new_container(
        LayoutStyle::new().width(40.0).height(40.0),
        &[canvas.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(40.0),
        AvailableSpace::Definite(40.0),
    )
    .unwrap();

    assert_eq!(fill_of(&canvas.view()), Paint::Solid(Color::RED));
    color.set(Color::BLUE);
    assert_eq!(
        fill_of(&canvas.view()),
        Paint::Solid(Color::BLUE),
        "draw closure must be re-read on the second view(), not cached from construction"
    );
}

/// Artwork that stands in for a glyph follows the region it is drawn in, the way the text beside it does. A caret drawn instead of spelled used to be the one mark in a region that had declared its ink that came out in the theme's.
#[test]
fn a_declaring_canvas_paints_with_the_ink_around_it() {
    reset_layout_runtime();
    let canvas = Canvas::declaring(LayoutStyle::new().width(40.0).height(40.0), |r, text| {
        RenderNode::rect(r, RectStyle::default().with_fill(text.color))
    })
    .unwrap();
    let root = new_container(
        LayoutStyle::new().width(40.0).height(40.0),
        &[canvas.layout_node()],
    )
    .unwrap();
    let declared = Color::rgba(0.9, 0.2, 0.1, 1.0);
    crate::declare(
        root,
        renderer_core::Declared::default().with_color(declared),
    );
    compute_layout(
        root,
        AvailableSpace::Definite(40.0),
        AvailableSpace::Definite(40.0),
    )
    .unwrap();

    assert_eq!(fill_of(&canvas.view()), Paint::Solid(declared));
}

fn fill_of(view: &RenderNode) -> Paint {
    let RenderNode::Transform { children, .. } = view else {
        panic!("expected Transform")
    };
    let RenderNode::Primitive(DrawCommand::Rect { style, .. }) = &children[0] else {
        panic!("expected a Rect primitive")
    };
    style.fill.expect("expected a fill")
}
