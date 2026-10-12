use super::*;
use crate::direction::Direction;
use crate::style::AlignItems;
use geometry_core::Size;

fn scroll_in(
    engine: &mut LayoutEngine,
    host_style: LayoutStyle,
    content: (f32, f32),
) -> (NodeId, NodeId, NodeId) {
    let content = engine
        .new_leaf(LayoutStyle::new().width(content.0).height(content.1))
        .unwrap();
    let host = engine.new_leaf(host_style).unwrap();
    engine.set_scroll_content(host, content).unwrap();
    let root = engine
        .new_container(
            LayoutStyle::new().flex_row().align_items(AlignItems::START),
            &[host],
        )
        .unwrap();
    engine
        .compute_layout(
            root,
            AvailableSpace::Definite(800.0),
            AvailableSpace::Definite(600.0),
        )
        .unwrap();
    (root, host, content)
}

#[test]
fn a_scroll_container_contributes_its_contents_size() {
    let mut engine = LayoutEngine::new();
    let (_, host, _) = scroll_in(&mut engine, LayoutStyle::new(), (240.0, 90.0));
    assert_eq!(
        engine.layout(host).unwrap(),
        geometry_core::Rect::new(0.0, 0.0, 240.0, 90.0)
    );
}

#[test]
fn a_scroll_containers_max_size_clamps_what_its_content_asks_for() {
    let mut engine = LayoutEngine::new();
    let (_, host, _) = scroll_in(
        &mut engine,
        LayoutStyle::new().max_width(100.0).max_height(50.0),
        (240.0, 90.0),
    );
    assert_eq!(
        engine.layout(host).unwrap(),
        geometry_core::Rect::new(0.0, 0.0, 100.0, 50.0)
    );
}

#[test]
fn a_walk_stops_at_scroll_content_and_hands_it_back_to_walk_from_the_origin() {
    let mut engine = LayoutEngine::new();
    let (root, host, content) = scroll_in(
        &mut engine,
        LayoutStyle::new().padding_all(10.0),
        (240.0, 90.0),
    );
    let mut met = Vec::new();
    let view = engine.root_rect(root).unwrap();
    let walked = engine
        .walk_in_view(root, view, &mut |node, _| {
            met.push(node);
            true
        })
        .unwrap();
    assert_eq!(met, [root, host]);
    assert_eq!(walked.scroll_contents, [content]);

    let mut placed = Vec::new();
    engine
        .walk_in_view(
            content,
            engine.root_rect(content).unwrap(),
            &mut |node, rect| {
                placed.push((node, rect));
                true
            },
        )
        .unwrap();
    assert_eq!(
        placed,
        [(content, geometry_core::Rect::new(0.0, 0.0, 240.0, 90.0))],
        "the content is placed in the space its scroll offset indexes into, not past the host's padding"
    );
}

#[test]
fn a_hidden_scroll_container_hands_back_no_content() {
    let mut engine = LayoutEngine::new();
    let (root, host, _) = scroll_in(&mut engine, LayoutStyle::new(), (240.0, 90.0));
    engine.set_display(host, false);
    lay_out_again(&mut engine, root);
    let walked = engine
        .walk_in_view(root, engine.root_rect(root).unwrap(), &mut |_, _| true)
        .unwrap();
    assert!(walked.scroll_contents.is_empty());
}

fn lay_out_again(engine: &mut LayoutEngine, root: NodeId) {
    engine.mark_dirty(root).unwrap();
    engine
        .compute_layout(
            root,
            AvailableSpace::Definite(800.0),
            AvailableSpace::Definite(600.0),
        )
        .unwrap();
}

#[test]
fn a_document_is_told_the_scroll_container_is_a_block_whatever_it_was_declared() {
    let mut engine = LayoutEngine::new();
    let content = engine.new_leaf(LayoutStyle::new()).unwrap();
    let host = engine
        .new_leaf(LayoutStyle::new().flex_row().flex_grow(1.0))
        .unwrap();
    engine.set_scroll_content(host, content).unwrap();
    let css = engine
        .declared_style(host)
        .unwrap()
        .to_css(Direction::Ltr, Size::ZERO);
    assert!(
        css.as_str().starts_with("display:block;"),
        "the browser lays the content out as Taffy does, as a block child: {css}"
    );
    assert!(css.as_str().contains("flex-grow:1"), "{css}");
}

#[test]
fn restyling_a_scroll_container_keeps_it_one() {
    let mut engine = LayoutEngine::new();
    let (root, host, _) = scroll_in(&mut engine, LayoutStyle::new(), (240.0, 90.0));
    engine
        .set_style(host, LayoutStyle::new().max_height(50.0))
        .unwrap();
    lay_out_again(&mut engine, root);
    assert_eq!(
        engine.layout(host).unwrap(),
        geometry_core::Rect::new(0.0, 0.0, 240.0, 50.0)
    );
}
