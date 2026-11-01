use layout_core::{AvailableSpace, LayoutStyle};
use renderer_core::{LayerMask, RectStyle, Role};
use ui_tree::RenderNode;

use super::*;
use crate::canvas::Canvas;
use crate::context::{compute_layout, reset_layout_runtime, track_layout};
use crate::layout_item::box_item;

fn block(style: LayoutStyle) -> Canvas {
    Canvas::new(style, |rect| {
        RenderNode::rect(rect, RectStyle::filled(renderer_core::Color::WHITE, 0.0))
    })
    .unwrap()
}

fn layer_roles(node: &RenderNode, out: &mut Vec<LayerMask>) {
    match node {
        RenderNode::Layer { mask, children, .. } => {
            out.push(*mask);
            children.iter().for_each(|child| layer_roles(child, out));
        }
        RenderNode::Element { children, .. }
        | RenderNode::Group { children }
        | RenderNode::Transform { children, .. }
        | RenderNode::Clip { children, .. } => {
            children.iter().for_each(|child| layer_roles(child, out))
        }
        _ => {}
    }
}

#[test]
fn the_source_lies_over_the_content_and_the_two_are_one_mask() {
    reset_layout_runtime();
    let source = block(LayoutStyle::new().height(20.0));
    let source_node = source.layout_node();
    let content = block(LayoutStyle::new().width(200.0).height(100.0));
    let mask = Mask::new(LayoutStyle::new(), box_item(source), box_item(content)).unwrap();
    compute_layout(
        mask.layout_node(),
        AvailableSpace::MaxContent,
        AvailableSpace::MaxContent,
    )
    .unwrap();
    let box_rect = track_layout(mask.layout_node()).unwrap().get();
    assert_eq!(
        (box_rect.width, box_rect.height),
        (200.0, 100.0),
        "the content sizes the box"
    );
    let over = track_layout(source_node).unwrap().get();
    assert_eq!(
        (over.y, over.width),
        (40.0, 200.0),
        "the source spans it, centred down its height"
    );

    let mut roles = Vec::new();
    layer_roles(&mask.view(), &mut roles);
    assert_eq!(roles, [LayerMask::Source, LayerMask::Apply]);
}

#[test]
fn a_document_draws_a_mask_as_one_picture() {
    reset_layout_runtime();
    let mask = Mask::new(
        LayoutStyle::new(),
        box_item(block(LayoutStyle::new().height(10.0))),
        box_item(block(LayoutStyle::new().width(40.0).height(40.0))),
    )
    .unwrap();
    let was = ui_tree::set_element_capture(true);
    let view = mask.view();
    ui_tree::set_element_capture(was);
    match view {
        RenderNode::Element { element, .. } => assert_eq!(element.semantics.role, Role::Drawing),
        _ => panic!("a document gets an element"),
    }
}
