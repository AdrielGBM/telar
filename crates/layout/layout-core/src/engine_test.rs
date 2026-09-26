use super::*;
use crate::style::SizeDimension;

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

/// A three-column, fixed-track grid to exercise explicit line placement against real taffy layout, not just the CSS dump.
fn three_column_grid(engine: &mut LayoutEngine, children: &[NodeId]) -> NodeId {
    use crate::track::TemplateTrack;
    let root = engine
        .new_container(
            LayoutStyle::new()
                .display_grid()
                .grid_template_columns(vec![
                    TemplateTrack::px(50.0),
                    TemplateTrack::px(50.0),
                    TemplateTrack::px(50.0),
                ])
                .width(150.0)
                .height(80.0),
            children,
        )
        .unwrap();
    lay_out(engine, root);
    root
}

#[test]
fn engine_grid_explicit_placement_puts_the_item_at_the_requested_cell() {
    let mut engine = LayoutEngine::new();
    let item = engine
        .new_leaf(LayoutStyle::new().grid_column(2, 1).grid_row(1, 1))
        .unwrap();
    three_column_grid(&mut engine, &[item]);

    let rect = engine.layout(item).unwrap();
    assert_eq!(rect.x, 50.0_f32, "line 2 is the start of the 2nd column");
    assert_eq!(rect.y, 0.0_f32);
    assert_eq!(rect.width, 50.0_f32, "one track wide, as requested");
}

#[test]
fn engine_grid_explicit_span_covers_tracks_from_its_start() {
    let mut engine = LayoutEngine::new();
    let item = engine
        .new_leaf(LayoutStyle::new().grid_column(1, 2))
        .unwrap();
    three_column_grid(&mut engine, &[item]);

    let rect = engine.layout(item).unwrap();
    assert_eq!(rect.x, 0.0_f32);
    assert_eq!(
        rect.width, 100.0_f32,
        "spans 2 tracks of 50px from column line 1"
    );
}

/// Taffy's auto-placement flows unplaced items into the first free cell scanning the grid, explicit items included or not: an item pinned to column 2 leaves column 1's cell free for the auto item that follows it in the tree, even though it comes second in document order.
#[test]
fn engine_grid_mixes_auto_and_explicit_cells() {
    let mut engine = LayoutEngine::new();
    let pinned = engine
        .new_leaf(
            LayoutStyle::new()
                .grid_column(2, 1)
                .grid_row(1, 1)
                .height(40.0),
        )
        .unwrap();
    let auto = engine.new_leaf(LayoutStyle::new().height(40.0)).unwrap();
    three_column_grid(&mut engine, &[pinned, auto]);

    let pinned_rect = engine.layout(pinned).unwrap();
    let auto_rect = engine.layout(auto).unwrap();
    assert_eq!(pinned_rect.x, 50.0_f32, "stays exactly where it was pinned");
    assert_eq!(
        auto_rect.x, 0.0_f32,
        "auto-placement flows into the free first column"
    );
    assert_eq!(auto_rect.y, pinned_rect.y, "same row: both are cell (1, *)");
}

/// Overlap detection is explicitly left to the caller: two items pinned to the same cell both land there rather than one being rejected or nudged.
#[test]
fn engine_grid_two_explicit_items_on_the_same_cell_overlap() {
    let mut engine = LayoutEngine::new();
    let a = engine
        .new_leaf(LayoutStyle::new().grid_column(1, 1).grid_row(1, 1))
        .unwrap();
    let b = engine
        .new_leaf(LayoutStyle::new().grid_column(1, 1).grid_row(1, 1))
        .unwrap();
    three_column_grid(&mut engine, &[a, b]);

    let rect_a = engine.layout(a).unwrap();
    let rect_b = engine.layout(b).unwrap();
    assert_eq!(rect_a.x, rect_b.x);
    assert_eq!(rect_a.y, rect_b.y);
    assert_eq!(rect_a.width, rect_b.width);
}

#[test]
fn engine_grid_explicit_row_template_sizes_the_row_an_item_lands_at() {
    use crate::track::TemplateTrack;
    let mut engine = LayoutEngine::new();
    let item = engine
        .new_leaf(LayoutStyle::new().grid_column(1, 1).grid_row(2, 1))
        .unwrap();
    let root = engine
        .new_container(
            LayoutStyle::new()
                .display_grid()
                .grid_template_columns(vec![TemplateTrack::px(100.0)])
                .grid_template_rows(vec![TemplateTrack::px(30.0), TemplateTrack::px(50.0)])
                .width(100.0)
                .height(80.0),
            &[item],
        )
        .unwrap();
    lay_out(&mut engine, root);

    let rect = engine.layout(item).unwrap();
    assert_eq!(
        rect.y, 30.0_f32,
        "row line 2 starts after the first template row (30px)"
    );
    assert_eq!(rect.height, 50.0_f32, "and stretches to fill it");
}

/// `grid_auto_rows` sizes the implicit rows an overflowing auto-placed item creates beyond the explicit template — here, the whole grid, since no `grid_template_rows` is given at all.
#[test]
fn engine_grid_auto_rows_sizes_implicit_rows() {
    use crate::track::TemplateTrack;
    let mut engine = LayoutEngine::new();
    let first = engine.new_leaf(LayoutStyle::new()).unwrap();
    let second = engine.new_leaf(LayoutStyle::new()).unwrap();
    let root = engine
        .new_container(
            LayoutStyle::new()
                .display_grid()
                .grid_template_columns(vec![TemplateTrack::px(100.0)])
                .grid_auto_rows(vec![TemplateTrack::px(40.0)])
                .width(100.0)
                .height(80.0),
            &[first, second],
        )
        .unwrap();
    lay_out(&mut engine, root);

    let first_rect = engine.layout(first).unwrap();
    let second_rect = engine.layout(second).unwrap();
    assert_eq!(
        first_rect.height, 40.0_f32,
        "the one-column grid forces both auto items into their own implicit row, sized by grid_auto_rows"
    );
    assert_eq!(
        second_rect.y, 40.0_f32,
        "the second implicit row starts after the first's 40px"
    );
    assert_eq!(second_rect.height, 40.0_f32);
}

fn surface_sized_leaf(engine: &mut LayoutEngine) -> (NodeId, NodeId, NodeId) {
    let fraction = engine
        .new_leaf(
            LayoutStyle::new()
                .width(SizeDimension::SurfaceWidth(0.5))
                .height(10.0),
        )
        .unwrap();
    let fixed = engine
        .new_leaf(LayoutStyle::new().width(40.0).height(10.0))
        .unwrap();
    let root = engine
        .new_container(
            LayoutStyle::new().flex_column().width(300.0).height(100.0),
            &[fraction, fixed],
        )
        .unwrap();
    (root, fraction, fixed)
}

#[test]
fn a_surface_fraction_follows_the_surface_not_the_parent() {
    let mut engine = LayoutEngine::new();
    let (root, fraction, _) = surface_sized_leaf(&mut engine);
    engine.set_surface_size(Size::new(1000.0, 600.0));
    lay_out(&mut engine, root);
    assert_eq!(engine.layout(fraction).unwrap().width, 500.0);

    assert!(engine.set_surface_size(Size::new(200.0, 600.0)));
    lay_out(&mut engine, root);
    assert_eq!(
        engine.layout(fraction).unwrap().width,
        100.0,
        "the resize alone dirtied it: nothing marked the tree by hand"
    );
}

#[test]
fn a_resize_leaves_nodes_that_do_not_name_the_surface_clean() {
    let mut engine = LayoutEngine::new();
    let fixed = engine
        .new_leaf(LayoutStyle::new().width(40.0).height(10.0))
        .unwrap();
    let root = engine
        .new_container(
            LayoutStyle::new().flex_column().width(300.0).height(100.0),
            &[fixed],
        )
        .unwrap();
    lay_out(&mut engine, root);
    assert!(
        !engine.set_surface_size(Size::new(800.0, 600.0)),
        "no style names the surface, so there is nothing to re-resolve"
    );
    assert!(!engine.is_dirty(root));
}

#[test]
fn the_same_surface_size_is_not_a_change() {
    let mut engine = LayoutEngine::new();
    let (root, _, _) = surface_sized_leaf(&mut engine);
    engine.set_surface_size(Size::new(640.0, 480.0));
    lay_out(&mut engine, root);
    assert!(!engine.set_surface_size(Size::new(640.0, 480.0)));
    assert!(!engine.is_dirty(root));
}

#[test]
fn filling_a_root_replaces_a_surface_width_it_was_declared_with() {
    let mut engine = LayoutEngine::new();
    let root = engine
        .new_leaf(LayoutStyle::new().width(SizeDimension::SurfaceWidth(0.5)))
        .unwrap();
    engine.set_width(root, Some(70.0));
    engine.set_surface_size(Size::new(1000.0, 600.0));
    lay_out(&mut engine, root);
    assert_eq!(engine.layout(root).unwrap().width, 70.0);
}

#[test]
fn a_grid_column_sized_by_the_surface_follows_a_resize() {
    use crate::track::TemplateTrack;

    let mut engine = LayoutEngine::new();
    let first = engine.new_leaf(LayoutStyle::new().height(10.0)).unwrap();
    let second = engine.new_leaf(LayoutStyle::new().height(10.0)).unwrap();
    let grid = engine
        .new_container(
            LayoutStyle::new()
                .display_grid()
                .width(300.0)
                .height(100.0)
                .grid_template_columns(vec![
                    TemplateTrack::length(SizeDimension::SurfaceWidth(0.1)),
                    TemplateTrack::fr(1.0),
                ]),
            &[first, second],
        )
        .unwrap();
    engine.set_surface_size(Size::new(1000.0, 600.0));
    lay_out(&mut engine, grid);
    assert_eq!(engine.layout(first).unwrap().width, 100.0);
    assert_eq!(engine.layout(second).unwrap().x, 100.0);

    assert!(engine.set_surface_size(Size::new(500.0, 600.0)));
    lay_out(&mut engine, grid);
    assert_eq!(engine.layout(first).unwrap().width, 50.0);
    assert_eq!(engine.layout(second).unwrap().width, 250.0);
}

/// A face arriving changes what text measures to without changing anything taffy can see, so its cached sizes have to be thrown away by hand — for the leaves that measure, and only those.
#[test]
fn marking_measured_leaves_stale_measures_them_again_and_leaves_the_rest_cached() {
    use std::cell::Cell;
    use std::rc::Rc;

    let width = Rc::new(Cell::new(40.0_f32));
    let measured_width = width.clone();
    let mut engine = LayoutEngine::new();
    let text = engine
        .new_measured_leaf(
            LayoutStyle::new(),
            Box::new(move |_| (measured_width.get(), 10.0)),
        )
        .unwrap();
    let fixed = engine
        .new_leaf(LayoutStyle::new().width(50.0).height(10.0))
        .unwrap();
    let root = engine
        .new_container(LayoutStyle::new().flex_row(), &[text, fixed])
        .unwrap();
    lay_out(&mut engine, root);
    assert_eq!(engine.layout(text).unwrap().width, 40.0);

    width.set(70.0);
    lay_out(&mut engine, root);
    assert_eq!(
        engine.layout(text).unwrap().width,
        40.0,
        "nothing said the measurement changed"
    );

    engine.mark_measured_dirty();
    assert!(engine.is_dirty(text) && !engine.is_dirty(fixed));
    lay_out(&mut engine, root);
    assert_eq!(engine.layout(text).unwrap().width, 70.0);
}

/// A row too narrow for its measured leaves squeezes each to its min-content width and no further — for text, its longest word — which a measure can only answer when it is told that min-content is the question.
#[test]
fn a_squeezed_row_stops_each_measured_leaf_at_its_min_content_width() {
    let word = || -> MeasureFn {
        Box::new(|width| match width {
            AvailableSpace::MinContent => (60.0, 20.0),
            AvailableSpace::MaxContent => (120.0, 10.0),
            AvailableSpace::Definite(w) if w >= 120.0 => (120.0, 10.0),
            AvailableSpace::Definite(w) => (w.max(60.0), 20.0),
        })
    };
    let mut engine = LayoutEngine::new();
    let first = engine
        .new_measured_leaf(LayoutStyle::new(), word())
        .unwrap();
    let second = engine
        .new_measured_leaf(LayoutStyle::new(), word())
        .unwrap();
    let root = engine
        .new_container(LayoutStyle::new().flex_row().width(100.0), &[first, second])
        .unwrap();
    lay_out(&mut engine, root);
    assert_eq!(engine.layout(first).unwrap().width, 60.0);
    assert_eq!(engine.layout(second).unwrap().width, 60.0);
}
