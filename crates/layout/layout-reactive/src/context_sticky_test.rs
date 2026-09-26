use geometry_core::Rect;
use layout_core::LayoutStyle;

use super::*;

struct Content {
    root: NodeId,
    bar: RwSignal<Rect>,
    label: RwSignal<Rect>,
    tail: RwSignal<Rect>,
}

/// A scroll's content, laid out on its own: a 100px header, then a sticky bar with a label, then 1000px more.
fn content() -> Content {
    reset_layout_runtime();
    let (label_node, label) = new_leaf(LayoutStyle::new().height(10.0)).unwrap();
    let bar_node = new_container(
        LayoutStyle::new().sticky().inset_top(0.0).height(40.0),
        &[label_node],
    )
    .unwrap();
    let bar = track_layout(bar_node).unwrap();
    let (header, _) = new_leaf(LayoutStyle::new().height(100.0)).unwrap();
    let (tail_node, tail) = new_leaf(LayoutStyle::new().height(1000.0)).unwrap();
    let root = new_container(
        LayoutStyle::new().flex_column(),
        &[header, bar_node, tail_node],
    )
    .unwrap();
    Content {
        root,
        bar,
        label,
        tail,
    }
}

fn lay_out(root: NodeId) {
    compute_layout(
        root,
        AvailableSpace::Definite(300.0),
        AvailableSpace::MaxContent,
    )
    .unwrap();
}

#[test]
fn a_root_with_no_view_sticks_against_its_own_box() {
    let page = content();
    lay_out(page.root);
    assert_eq!(page.bar.peek().y, 100.0);
}

#[test]
fn a_new_view_moves_the_sticky_subtree_without_a_layout_pass() {
    let page = content();
    lay_out(page.root);
    let passes = layout_passes();

    set_sticky_view(page.root, Rect::new(0.0, 250.0, 300.0, 200.0));

    assert_eq!(page.bar.peek().y, 250.0);
    assert_eq!(page.label.peek().y, 250.0, "its subtree moves with it");
    assert_eq!(
        page.tail.peek().y,
        140.0,
        "the rest stays where layout put it"
    );
    assert_eq!(layout_passes(), passes, "nothing was laid out again");
}

#[test]
fn a_relayout_keeps_the_view_it_was_given() {
    let page = content();
    set_sticky_view(page.root, Rect::new(0.0, 250.0, 300.0, 200.0));
    lay_out(page.root);
    assert_eq!(page.bar.peek().y, 250.0);
    set_sticky_view(page.root, Rect::new(0.0, 0.0, 300.0, 200.0));
    assert_eq!(
        page.bar.peek().y,
        100.0,
        "and scrolling back puts it where it was"
    );
}

#[test]
fn a_freed_root_forgets_its_view_and_its_anchors() {
    let page = content();
    set_sticky_view(page.root, Rect::new(0.0, 250.0, 300.0, 200.0));
    lay_out(page.root);
    remove_node(page.root);
    with_runtime_ref(|rt| {
        assert!(rt.sticky_views.is_empty());
        assert!(rt.sticky_anchors.is_empty());
    });
}
