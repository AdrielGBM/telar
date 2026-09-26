use super::*;
use geometry_core::Rect;

/// A 400-wide scroll content: a 100px header, a 600px section holding a 50px sticky bar with a child, and 1000px after it.
struct Page {
    engine: LayoutEngine,
    root: NodeId,
    section: NodeId,
    bar: NodeId,
    label: NodeId,
}

fn page(bar: LayoutStyle) -> Page {
    let mut engine = LayoutEngine::new();
    let label = engine
        .new_leaf(LayoutStyle::new().width(40.0).height(10.0))
        .unwrap();
    let bar = engine
        .new_container(bar.height(50.0).padding_all(5.0), &[label])
        .unwrap();
    let section = engine
        .new_container(
            LayoutStyle::new()
                .flex_column()
                .height(600.0)
                .padding_top(20.0),
            &[bar],
        )
        .unwrap();
    let header = engine.new_leaf(LayoutStyle::new().height(100.0)).unwrap();
    let tail = engine.new_leaf(LayoutStyle::new().height(1000.0)).unwrap();
    let root = engine
        .new_container(
            LayoutStyle::new().flex_column().width(400.0),
            &[header, section, tail],
        )
        .unwrap();
    engine
        .compute_layout(
            root,
            AvailableSpace::Definite(400.0),
            AvailableSpace::MaxContent,
        )
        .unwrap();
    Page {
        engine,
        root,
        section,
        bar,
        label,
    }
}

fn scrolled(y: f32) -> Rect {
    Rect::new(0.0, y, 400.0, 300.0)
}

fn rects_in_view(page: &Page, view: Rect) -> (FxHashMap<NodeId, Rect>, Vec<StickyAnchor>) {
    let mut rects = FxHashMap::default();
    let anchors = page
        .engine
        .walk_in_view(page.root, view, &mut |node, rect| {
            rects.insert(node, rect);
            true
        })
        .unwrap();
    (rects, anchors)
}

#[test]
fn a_sticky_box_is_laid_out_in_the_flow() {
    let page = page(LayoutStyle::new().sticky().inset_top(0.0));
    let (rects, _) = rects_in_view(&page, scrolled(0.0));
    assert_eq!(
        rects[&page.bar].y, 120.0,
        "under the header and the padding"
    );
    assert_eq!(
        page.engine.layout(page.bar).unwrap().y,
        20.0,
        "and taffy never reads its inset as a relative offset"
    );
}

#[test]
fn scrolling_past_it_carries_it_and_its_subtree() {
    let page = page(LayoutStyle::new().sticky().inset_top(10.0));
    let (rects, anchors) = rects_in_view(&page, scrolled(300.0));
    assert_eq!(rects[&page.bar].y, 310.0);
    assert_eq!(rects[&page.label].y, 315.0, "the child moves with it");
    assert_eq!(rects[&page.section].y, 100.0, "its parent does not");
    assert_eq!(
        anchors.iter().map(StickyAnchor::node).collect::<Vec<_>>(),
        vec![page.bar]
    );
}

#[test]
fn it_stops_at_the_end_of_its_containing_block() {
    let page = page(LayoutStyle::new().sticky().inset_top(0.0));
    let (rects, _) = rects_in_view(&page, scrolled(1200.0));
    assert_eq!(rects[&page.bar].y, 100.0 + 600.0 - 50.0);
}

#[test]
fn an_anchor_places_its_subtree_again_for_a_new_view() {
    let page = page(LayoutStyle::new().sticky().inset_top(0.0));
    let (_, anchors) = rects_in_view(&page, scrolled(0.0));
    let mut moved = FxHashMap::default();
    page.engine
        .walk_anchor(&anchors[0], scrolled(400.0), &mut |node, rect| {
            moved.insert(node, rect);
            true
        })
        .unwrap();
    let (walked, _) = rects_in_view(&page, scrolled(400.0));
    assert_eq!(moved.len(), 2, "the bar and its label, nothing else");
    for (node, rect) in moved {
        assert_eq!(rect, walked[&node], "the same place a full walk puts it");
    }
}

#[test]
fn walk_leaves_a_sticky_box_where_layout_put_it() {
    let page = page(LayoutStyle::new().sticky().inset_top(0.0));
    let mut bar = None;
    page.engine
        .walk(page.root, &mut |node, rect| {
            if node == page.bar {
                bar = Some(rect);
            }
            true
        })
        .unwrap();
    assert_eq!(bar.unwrap().y, 120.0);
}

#[test]
fn a_hidden_sticky_box_does_not_stick() {
    let page = page(LayoutStyle::new().sticky().inset_top(0.0).display_none());
    let (_, anchors) = rects_in_view(&page, scrolled(300.0));
    assert!(anchors.is_empty());
}

#[test]
fn a_style_that_stops_sticking_is_forgotten() {
    let mut page = page(LayoutStyle::new().sticky().inset_top(0.0));
    page.engine
        .set_style(page.bar, LayoutStyle::new().height(50.0))
        .unwrap();
    let (rects, anchors) = rects_in_view(&page, scrolled(300.0));
    assert!(anchors.is_empty());
    assert_eq!(rects[&page.bar].y, 120.0);
}

#[test]
fn a_removed_sticky_node_is_forgotten() {
    let mut page = page(LayoutStyle::new().sticky().inset_top(0.0));
    page.engine.set_children(page.section, &[]).unwrap();
    page.engine.remove(page.bar);
    assert!(page.engine.sticky.is_empty());
}
