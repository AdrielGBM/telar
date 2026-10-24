use std::cell::Cell;
use std::rc::Rc;

use layout_core::AvailableSpace;
use renderer_core::{DrawCommand, Paint};

use super::*;
use crate::context::{compute_layout, new_container, reset_layout_runtime};
use crate::layout_item::LayoutItem;

fn make_svg_data(w: f32, h: f32) -> Arc<SvgData> {
    let src = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}"><rect width="{w}" height="{h}" fill="#ff0000"/></svg>"##
    );
    Arc::new(SvgData::from_str(&src).unwrap())
}

#[test]
fn svg_without_size_uses_intrinsic_size() {
    reset_layout_runtime();
    let data = make_svg_data(24.0, 16.0);
    let svg = Svg::new(
        LayoutStyle::new(),
        move || Arc::clone(&data),
        || None,
        || ObjectFit::Contain,
    )
    .unwrap();
    let root = new_container(
        LayoutStyle::new().flex_column().width(200.0).height(200.0),
        &[svg.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();

    let rect = svg.leaf.rect.get();
    assert_eq!(rect.width, 24.0);
    assert_eq!(rect.height, 16.0);
}

#[test]
fn svg_with_only_width_derives_height_from_aspect_ratio() {
    reset_layout_runtime();
    let data = make_svg_data(24.0, 16.0); // 1.5:1 aspect ratio
    let svg = Svg::new(
        LayoutStyle::new().width(48.0),
        move || Arc::clone(&data),
        || None,
        || ObjectFit::Contain,
    )
    .unwrap();
    let root = new_container(
        LayoutStyle::new().flex_column().width(200.0).height(200.0),
        &[svg.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();

    let rect = svg.leaf.rect.get();
    assert_eq!(rect.width, 48.0);
    assert_eq!(rect.height, 32.0);
}

#[test]
fn svg_view_emits_group_with_path_primitive() {
    reset_layout_runtime();
    let data = make_svg_data(10.0, 10.0);
    let svg = Svg::new(
        LayoutStyle::new().width(20.0).height(20.0),
        move || Arc::clone(&data),
        || None,
        || ObjectFit::Contain,
    )
    .unwrap();
    let root = new_container(
        LayoutStyle::new().width(20.0).height(20.0),
        &[svg.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(20.0),
        AvailableSpace::Definite(20.0),
    )
    .unwrap();

    let view = svg.view();
    let RenderNode::Element { children, .. } = &view else {
        panic!("expected the svg's element")
    };
    let RenderNode::Transform { children, .. } = &children[0] else {
        panic!("expected Transform")
    };
    assert_eq!(children.len(), 1);
    let RenderNode::Group { children: inner } = &children[0] else {
        panic!("expected Group inside Transform")
    };
    assert!(
        inner
            .iter()
            .any(|n| matches!(n, RenderNode::Primitive(DrawCommand::Path { .. }))),
        "expected at least one Path primitive among {} children",
        inner.len()
    );
}

#[test]
fn tint_closure_is_re_read_each_view_and_recolors_the_path() {
    reset_layout_runtime();
    let data = make_svg_data(10.0, 10.0);
    let tint = Rc::new(Cell::new(Color::GREEN));
    let tint_read = tint.clone();
    let svg = Svg::new(
        LayoutStyle::new().width(20.0).height(20.0),
        move || Arc::clone(&data),
        move || Some(tint_read.get()),
        || ObjectFit::Contain,
    )
    .unwrap();
    let root = new_container(
        LayoutStyle::new().width(20.0).height(20.0),
        &[svg.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(20.0),
        AvailableSpace::Definite(20.0),
    )
    .unwrap();

    assert_eq!(path_fill(&svg.view()), Paint::Solid(Color::GREEN));
    tint.set(Color::BLUE);
    assert_eq!(
        path_fill(&svg.view()),
        Paint::Solid(Color::BLUE),
        "tint closure must be re-read on the second view(), not cached from construction"
    );
}

#[test]
fn data_closure_is_re_read_each_view_and_swaps_the_glyph() {
    reset_layout_runtime();
    let one = Arc::new(
        SvgData::from_str(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="4" height="4" fill="#f00"/></svg>"##,
        )
        .unwrap(),
    );
    let two = Arc::new(
        SvgData::from_str(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="4" height="4" fill="#f00"/><rect x="5" y="5" width="4" height="4" fill="#0f0"/></svg>"##,
        )
        .unwrap(),
    );
    let pick_two = Rc::new(Cell::new(false));
    let (pick, a, b) = (pick_two.clone(), one.clone(), two.clone());
    let svg = Svg::new(
        LayoutStyle::new().width(20.0).height(20.0),
        move || Arc::clone(if pick.get() { &b } else { &a }),
        || None,
        || ObjectFit::Contain,
    )
    .unwrap();
    let root = new_container(
        LayoutStyle::new().width(20.0).height(20.0),
        &[svg.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(20.0),
        AvailableSpace::Definite(20.0),
    )
    .unwrap();

    assert_eq!(count_paths(&svg.view()), 1, "first glyph has one shape");
    pick_two.set(true);
    assert_eq!(
        count_paths(&svg.view()),
        2,
        "data closure must be re-read on the second view(), swapping to the two-shape glyph"
    );
}

// Regression: the second frame must hold only the new glyph's paths — a leftover from the first was the "icon after the icon" duplicate seen live.
#[test]
fn reactive_svg_swap_through_component_list_replaces() {
    use reactive_core::signal;
    reset_layout_runtime();
    let one = Arc::new(
        SvgData::from_str(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="4" height="4" fill="#f00"/></svg>"##,
        )
        .unwrap(),
    );
    let two = Arc::new(
        SvgData::from_str(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="4" height="4" fill="#f00"/><rect x="5" y="5" width="4" height="4" fill="#0f0"/></svg>"##,
        )
        .unwrap(),
    );
    let pick_two = signal(false);
    let (pick, a, b) = (pick_two, one.clone(), two.clone());
    let svg = Svg::new(
        LayoutStyle::new().width(20.0).height(20.0),
        move || Arc::clone(if pick.get() { &b } else { &a }),
        || None,
        || ObjectFit::Contain,
    )
    .unwrap();
    let node = svg.layout_node();
    let root = new_container(LayoutStyle::new().width(20.0).height(20.0), &[node]).unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(20.0),
        AvailableSpace::Definite(20.0),
    )
    .unwrap();

    let tree = crate::ComponentList::new(svg);
    let count = |cmds: &[DrawCommand]| {
        cmds.iter()
            .filter(|c| matches!(c, DrawCommand::Path { .. }))
            .count()
    };
    assert_eq!(count(&tree.commands()), 1, "first frame: one-shape glyph");
    pick_two.set(true);
    assert_eq!(
        count(&tree.commands()),
        2,
        "second frame must be ONLY the two-shape glyph, not stacked on the first"
    );
}

fn count_paths(view: &RenderNode) -> usize {
    let RenderNode::Element { children, .. } = &view else {
        panic!("expected the svg's element")
    };
    let RenderNode::Transform { children, .. } = &children[0] else {
        panic!("expected Transform")
    };
    let RenderNode::Group { children: inner } = &children[0] else {
        panic!("expected Group inside Transform")
    };
    inner
        .iter()
        .filter(|n| matches!(n, RenderNode::Primitive(DrawCommand::Path { .. })))
        .count()
}

fn path_fill(view: &RenderNode) -> Paint {
    let RenderNode::Element { children, .. } = &view else {
        panic!("expected the svg's element")
    };
    let RenderNode::Transform { children, .. } = &children[0] else {
        panic!("expected Transform")
    };
    let RenderNode::Group { children: inner } = &children[0] else {
        panic!("expected Group inside Transform")
    };
    inner
        .iter()
        .find_map(|n| match n {
            RenderNode::Primitive(DrawCommand::Path { style, .. }) => style.fill,
            _ => None,
        })
        .expect("expected a filled Path primitive")
}
