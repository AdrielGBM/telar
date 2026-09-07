use super::*;

fn lay_out(engine: &mut LayoutEngine, root: NodeId) {
    engine
        .compute_layout(
            root,
            AvailableSpace::Definite(300.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();
}

/// Two owners can reach the same node without seeing each other — a list reconciling away a row it no longer has, and that row's own owner being disposed — and both are right to free what they held. Taffy panics on a key it has already handed back, so the second free took the process down with it.
#[test]
fn freeing_a_node_twice_is_a_no_op() {
    let mut engine = LayoutEngine::new();
    let node = engine.new_leaf(LayoutStyle::new()).unwrap();
    engine.remove(node);
    engine.remove(node);
    assert!(engine.mark_dirty(node).is_err(), "and it stays gone");
}

/// Every mutator answers for a freed node instead of taking the process down with it.
///
/// Taffy indexes its slot map directly — `style()` included, so there is nothing to guard with but our own record — and a widget can outlive its node by design: a `Segment` holds it so a re-render mid-dispatch can still flatten it, so an effect it owns may fire once after the node is gone. These all return `Result`; an error is what the signature already promised.
#[test]
fn a_freed_node_is_an_error_and_never_a_panic() {
    let mut engine = LayoutEngine::new();
    let parent = engine.new_container(LayoutStyle::new(), &[]).unwrap();
    let child = engine.new_leaf(LayoutStyle::new()).unwrap();
    let ghost = engine.new_leaf(LayoutStyle::new()).unwrap();
    engine.remove(ghost);

    assert!(
        engine.set_children(ghost, &[]).is_err(),
        "set_children on a freed node is an error, not a panic"
    );
    assert!(
        engine.set_children(parent, &[ghost]).is_err(),
        "nor may a freed node be adopted as a child"
    );
    assert!(
        engine.set_style(ghost, LayoutStyle::new()).is_err(),
        "set_style on a freed node is an error, not a panic"
    );
    assert!(
        engine.mark_dirty(ghost).is_err(),
        "mark_dirty on a freed node is an error, not a panic"
    );
    assert!(
        engine.add_child(parent, ghost).is_err(),
        "add_child with a freed child is an error, not a panic"
    );
    assert!(
        engine.remove_child(parent, ghost).is_err(),
        "remove_child with a freed child is an error, not a panic"
    );
    assert!(
        engine.layout(ghost).is_err(),
        "laying out a freed node is an error, not a panic"
    );
    assert!(
        engine
            .compute_layout(
                ghost,
                AvailableSpace::MaxContent,
                AvailableSpace::MaxContent
            )
            .is_err(),
        "computing layout for a freed node is an error, not a panic"
    );
    assert_eq!(engine.is_size_auto(ghost), (false, false));
    assert!(
        engine.is_fixed_size(ghost).is_none(),
        "a freed node has no size to report"
    );
    // The reads that cannot fail still have to answer without touching taffy.
    engine.set_width(ghost, Some(10.0));
    engine.set_height(ghost, None);
    engine.set_leading_margin(ghost, true, 4.0);

    // And the live node beside it is untouched by any of that.
    assert!(
        engine.set_children(parent, &[child]).is_ok(),
        "a live parent and a live child is the ordinary case"
    );
}

#[test]
fn flipping_direction_relays_an_existing_row_without_rebuilding_it() {
    // The whole point of resolving late: the same nodes, laid out the other way round.
    let mut engine = LayoutEngine::new();
    let first = engine
        .new_leaf(LayoutStyle::new().width(50.0).height(10.0))
        .unwrap();
    let second = engine
        .new_leaf(LayoutStyle::new().width(50.0).height(10.0))
        .unwrap();
    let row = engine
        .new_container(
            LayoutStyle::new().flex_row().width(300.0).height(100.0),
            &[first, second],
        )
        .unwrap();
    lay_out(&mut engine, row);
    assert_eq!(engine.layout(first).unwrap().x, 0.0);
    assert_eq!(engine.layout(second).unwrap().x, 50.0);

    assert!(
        engine.set_direction(Direction::Rtl),
        "flipping to Rtl is a change"
    );
    engine.mark_dirty(row).unwrap();
    lay_out(&mut engine, row);
    assert_eq!(
        engine.layout(first).unwrap().x,
        250.0,
        "the first item now starts at the right edge"
    );
    assert_eq!(engine.layout(second).unwrap().x, 200.0);
}

#[test]
fn flipping_direction_moves_logical_padding_to_the_other_edge() {
    let mut engine = LayoutEngine::new();
    let child = engine
        .new_leaf(LayoutStyle::new().width(50.0).height(10.0))
        .unwrap();
    let box_ = engine
        .new_container(
            LayoutStyle::new()
                .flex_column()
                .width(300.0)
                .height(100.0)
                .padding_start(20.0),
            &[child],
        )
        .unwrap();
    lay_out(&mut engine, box_);
    assert_eq!(engine.layout(child).unwrap().x, 20.0);

    engine.set_direction(Direction::Rtl);
    engine.mark_dirty(box_).unwrap();
    lay_out(&mut engine, box_);
    assert_eq!(
        engine.layout(child).unwrap().x,
        0.0,
        "padding moved to the right edge, so the child starts flush left"
    );
}

#[test]
fn setting_the_same_direction_reports_no_change() {
    let mut engine = LayoutEngine::new();
    assert!(
        !engine.set_direction(Direction::Ltr),
        "the engine is already Ltr, so nothing changed"
    );
    assert!(engine.set_direction(Direction::Rtl), "Rtl is a change");
    assert!(
        !engine.set_direction(Direction::Rtl),
        "and setting Rtl again is not"
    );
}

#[test]
fn restyling_a_node_drops_the_logical_edges_it_no_longer_has() {
    // A stale entry would keep re-applying the old padding on every flip.
    let mut engine = LayoutEngine::new();
    let node = engine
        .new_leaf(LayoutStyle::new().padding_start(20.0).width(50.0))
        .unwrap();
    engine
        .set_style(node, LayoutStyle::new().width(50.0))
        .unwrap();
    engine.set_direction(Direction::Rtl);
    let style = engine.tree.style(node).unwrap();
    assert_eq!(style.padding.left, taffy::LengthPercentage::length(0.0));
    assert_eq!(style.padding.right, taffy::LengthPercentage::length(0.0));
}

#[test]
fn a_gap_margin_follows_the_edge_its_row_leads_from() {
    let mut engine = LayoutEngine::new();
    let first = engine.new_leaf(LayoutStyle::new().width(50.0)).unwrap();
    let second = engine.new_leaf(LayoutStyle::new().width(50.0)).unwrap();
    let row = engine
        .new_container(LayoutStyle::new().flex_row().width(300.0), &[first, second])
        .unwrap();
    engine.set_leading_margin(second, true, 8.0);
    assert_eq!(
        engine.tree.style(second).unwrap().margin.left,
        taffy::LengthPercentageAuto::length(8.0)
    );

    engine.set_direction(Direction::Rtl);
    engine.mark_dirty(row).unwrap();
    let margin = engine.tree.style(second).unwrap().margin;
    assert_eq!(
        margin.right,
        taffy::LengthPercentageAuto::length(8.0),
        "the gap moved to the edge the reversed row leads from"
    );
    assert_eq!(
        margin.left,
        taffy::LengthPercentageAuto::length(0.0),
        "and does not linger on the old one"
    );
}

#[test]
fn engine_leaf_layout() {
    let mut engine = LayoutEngine::new();
    let leaf = engine
        .new_leaf(LayoutStyle::new().width(50.0).height(40.0))
        .unwrap();
    engine
        .compute_layout(
            leaf,
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(200.0),
        )
        .unwrap();
    let rect = engine.layout(leaf).unwrap();
    assert_eq!(rect.width, 50.0_f32);
    assert_eq!(rect.height, 40.0_f32);
}

#[test]
fn engine_flex_row_positions() {
    let mut engine = LayoutEngine::new();
    let child1 = engine
        .new_leaf(LayoutStyle::new().width(100.0).height(100.0))
        .unwrap();
    let child2 = engine
        .new_leaf(LayoutStyle::new().width(100.0).height(100.0))
        .unwrap();
    let root = engine
        .new_container(
            LayoutStyle::new().flex_row().width(200.0).height(100.0),
            &[child1, child2],
        )
        .unwrap();
    engine
        .compute_layout(
            root,
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();

    let r1 = engine.layout(child1).unwrap();
    let r2 = engine.layout(child2).unwrap();
    assert_eq!(r1.x, 0.0_f32);
    assert_eq!(r1.y, 0.0_f32);
    assert_eq!(r2.x, 100.0_f32);
    assert_eq!(r2.y, 0.0_f32);
}

#[test]
fn engine_flex_column_positions() {
    let mut engine = LayoutEngine::new();
    let child1 = engine
        .new_leaf(LayoutStyle::new().width(100.0).height(100.0))
        .unwrap();
    let child2 = engine
        .new_leaf(LayoutStyle::new().width(100.0).height(100.0))
        .unwrap();
    let root = engine
        .new_container(
            LayoutStyle::new().flex_column().width(100.0).height(200.0),
            &[child1, child2],
        )
        .unwrap();
    engine
        .compute_layout(
            root,
            AvailableSpace::Definite(100.0),
            AvailableSpace::Definite(200.0),
        )
        .unwrap();

    let r1 = engine.layout(child1).unwrap();
    let r2 = engine.layout(child2).unwrap();
    assert_eq!(r1.x, 0.0_f32);
    assert_eq!(r1.y, 0.0_f32);
    assert_eq!(r2.x, 0.0_f32);
    assert_eq!(r2.y, 100.0_f32);
}

#[test]
fn engine_walk_absolute() {
    let mut engine = LayoutEngine::new();
    let inner_child = engine
        .new_leaf(LayoutStyle::new().width(50.0).height(50.0))
        .unwrap();
    let inner = engine
        .new_container(
            LayoutStyle::new().flex_row().width(50.0).height(50.0),
            &[inner_child],
        )
        .unwrap();
    let outer_first = engine
        .new_leaf(LayoutStyle::new().width(100.0).height(50.0))
        .unwrap();
    let root = engine
        .new_container(
            LayoutStyle::new().flex_row().width(150.0).height(50.0),
            &[outer_first, inner],
        )
        .unwrap();
    engine
        .compute_layout(
            root,
            AvailableSpace::Definite(150.0),
            AvailableSpace::Definite(50.0),
        )
        .unwrap();

    let mut hits: Vec<(NodeId, geometry_core::Rect)> = Vec::new();
    engine
        .walk(root, &mut |node, rect| {
            hits.push((node, rect));
            true
        })
        .unwrap();

    let inner_child_rect = hits
        .iter()
        .find(|(n, _)| *n == inner_child)
        .map(|(_, r)| *r)
        .unwrap();
    assert_eq!(inner_child_rect.x, 100.0_f32);
    assert_eq!(inner_child_rect.y, 0.0_f32);
    assert_eq!(inner_child_rect.width, 50.0_f32);
    assert_eq!(inner_child_rect.height, 50.0_f32);
}

#[test]
fn engine_set_style() {
    let mut engine = LayoutEngine::new();
    let leaf = engine
        .new_leaf(LayoutStyle::new().width(10.0).height(10.0))
        .unwrap();
    engine
        .set_style(leaf, LayoutStyle::new().width(80.0).height(60.0))
        .unwrap();
    engine
        .compute_layout(
            leaf,
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(200.0),
        )
        .unwrap();
    let rect = engine.layout(leaf).unwrap();
    assert_eq!(rect.width, 80.0_f32);
    assert_eq!(rect.height, 60.0_f32);
}
