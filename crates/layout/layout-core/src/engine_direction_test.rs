use super::*;
use crate::style::AlignItems;
use crate::track::TemplateTrack;

const WIDTH: f32 = 300.0;

fn lay_out(engine: &mut LayoutEngine, root: NodeId) {
    engine
        .compute_layout(
            root,
            AvailableSpace::Definite(WIDTH),
            AvailableSpace::Definite(200.0),
        )
        .unwrap();
}

fn x_under(direction: Direction, build: impl Fn(&mut LayoutEngine) -> (NodeId, NodeId)) -> f32 {
    let mut engine = LayoutEngine::new();
    engine.set_direction(direction);
    let (root, probe) = build(&mut engine);
    lay_out(&mut engine, root);
    engine.layout(probe).unwrap().x
}

fn sized(engine: &mut LayoutEngine, width: f32) -> NodeId {
    engine
        .new_leaf(LayoutStyle::new().width(width).height(20.0))
        .unwrap()
}

/// A column's cross axis is the inline axis, so its start is the inline start: the right edge under RTL, as CSS has it.
#[test]
fn a_column_aligned_to_its_start_starts_at_the_right_under_rtl() {
    for align in [AlignItems::FLEX_START, AlignItems::START] {
        let build = |engine: &mut LayoutEngine| {
            let child = sized(engine, 50.0);
            let column = engine
                .new_container(
                    LayoutStyle::new()
                        .flex_column()
                        .width(WIDTH)
                        .align_items(align),
                    &[child],
                )
                .unwrap();
            (column, child)
        };
        assert_eq!(x_under(Direction::Ltr, build), 0.0, "{align:?}");
        assert_eq!(x_under(Direction::Rtl, build), 250.0, "{align:?}");
    }
}

#[test]
fn a_column_aligned_to_its_end_ends_at_the_left_under_rtl() {
    let build = |engine: &mut LayoutEngine| {
        let child = sized(engine, 50.0);
        let column = engine
            .new_container(
                LayoutStyle::new()
                    .flex_column()
                    .width(WIDTH)
                    .align_items(AlignItems::FLEX_END),
                &[child],
            )
            .unwrap();
        (column, child)
    };
    assert_eq!(x_under(Direction::Ltr, build), 250.0);
    assert_eq!(x_under(Direction::Rtl, build), 0.0);
}

#[test]
fn align_self_start_in_a_column_mirrors_under_rtl() {
    let build = |engine: &mut LayoutEngine| {
        let child = engine
            .new_leaf(
                LayoutStyle::new()
                    .width(50.0)
                    .height(20.0)
                    .align_self_start(),
            )
            .unwrap();
        let column = engine
            .new_container(
                LayoutStyle::new()
                    .flex_column()
                    .width(WIDTH)
                    .align_items(AlignItems::CENTER),
                &[child],
            )
            .unwrap();
        (column, child)
    };
    assert_eq!(x_under(Direction::Ltr, build), 0.0);
    assert_eq!(x_under(Direction::Rtl, build), 250.0);
}

/// A fixed-width child cannot stretch, so the default alignment falls back to the start, which mirrors with it.
#[test]
fn a_fixed_width_child_of_a_stretching_column_sits_at_the_inline_start() {
    let build = |engine: &mut LayoutEngine| {
        let child = sized(engine, 50.0);
        let column = engine
            .new_container(LayoutStyle::new().flex_column().width(WIDTH), &[child])
            .unwrap();
        (column, child)
    };
    assert_eq!(x_under(Direction::Ltr, build), 0.0);
    assert_eq!(x_under(Direction::Rtl, build), 250.0);
}

/// The shape of a padded preview's page: a padded column aligned to its start, holding a column the preview's own size. Under RTL the preview belongs in the top-right corner, inside the padding.
#[test]
fn a_padded_page_places_its_preview_at_the_inline_start_corner() {
    let build = |engine: &mut LayoutEngine| {
        let preview = sized(engine, 80.0);
        let mount = engine
            .new_container(LayoutStyle::new().flex_column(), &[preview])
            .unwrap();
        let page = engine
            .new_container(
                LayoutStyle::new()
                    .flex_column()
                    .width(WIDTH)
                    .height(200.0)
                    .padding_all(16.0)
                    .align_items(AlignItems::FLEX_START),
                &[mount],
            )
            .unwrap();
        (page, mount)
    };
    assert_eq!(x_under(Direction::Ltr, build), 16.0);
    assert_eq!(x_under(Direction::Rtl, build), WIDTH - 16.0 - 80.0);
}

/// A box left to the default display places a narrower child at the inline start too.
#[test]
fn a_block_places_a_narrower_child_at_the_inline_start() {
    let build = |engine: &mut LayoutEngine| {
        let child = sized(engine, 50.0);
        let block = engine
            .new_container(LayoutStyle::new().width(WIDTH), &[child])
            .unwrap();
        (block, child)
    };
    assert_eq!(x_under(Direction::Ltr, build), 0.0);
    assert_eq!(x_under(Direction::Rtl, build), 250.0);
}

/// Grid columns run along the inline axis, so the first column is the rightmost under RTL.
#[test]
fn a_grid_places_its_first_column_at_the_inline_start() {
    let build = |engine: &mut LayoutEngine| {
        let first = engine.new_leaf(LayoutStyle::new().height(20.0)).unwrap();
        let second = engine.new_leaf(LayoutStyle::new().height(20.0)).unwrap();
        let grid = engine
            .new_container(
                LayoutStyle::new()
                    .display_grid()
                    .width(WIDTH)
                    .grid_template_columns(vec![
                        TemplateTrack::px(100.0),
                        TemplateTrack::px(200.0),
                    ]),
                &[first, second],
            )
            .unwrap();
        (grid, first)
    };
    assert_eq!(x_under(Direction::Ltr, build), 0.0);
    assert_eq!(x_under(Direction::Rtl, build), 200.0);
}

/// Taffy is told the direction, so a row is not also reversed for it: the first item of an RTL row is the rightmost, once.
#[test]
fn an_rtl_row_starts_at_the_right_exactly_once() {
    let build = |engine: &mut LayoutEngine| {
        let first = sized(engine, 50.0);
        let second = sized(engine, 50.0);
        let row = engine
            .new_container(LayoutStyle::new().flex_row().width(WIDTH), &[first, second])
            .unwrap();
        (row, first)
    };
    assert_eq!(x_under(Direction::Ltr, build), 0.0);
    assert_eq!(x_under(Direction::Rtl, build), 250.0);
}

#[test]
fn an_explicitly_reversed_row_runs_right_to_left_in_both_directions() {
    let build = |engine: &mut LayoutEngine| {
        let first = sized(engine, 50.0);
        let second = sized(engine, 50.0);
        let row = engine
            .new_container(
                LayoutStyle::new().flex_row_reverse().width(WIDTH),
                &[first, second],
            )
            .unwrap();
        (row, first)
    };
    assert_eq!(x_under(Direction::Ltr, build), 250.0);
    assert_eq!(x_under(Direction::Rtl, build), 250.0);
}

/// A logical start alignment of a row's main axis is the inline start, as a flex one is.
#[test]
fn a_row_justified_to_its_logical_start_starts_at_the_right_under_rtl() {
    let build = |engine: &mut LayoutEngine| {
        let child = sized(engine, 50.0);
        let row = engine
            .new_container(
                LayoutStyle::new()
                    .flex_row()
                    .width(WIDTH)
                    .justify_content(crate::style::JustifyContent::START),
                &[child],
            )
            .unwrap();
        (row, child)
    };
    assert_eq!(x_under(Direction::Ltr, build), 0.0);
    assert_eq!(x_under(Direction::Rtl, build), 250.0);
}

/// A box placed at a physical x stays at it under RTL, though the column it sits in now starts from the right.
#[test]
fn a_box_placed_from_the_left_stays_there_under_rtl() {
    let build = |engine: &mut LayoutEngine| {
        let panel = engine
            .new_leaf(
                LayoutStyle::new()
                    .width(80.0)
                    .height(40.0)
                    .margin_from_left(40.0),
            )
            .unwrap();
        let backdrop = engine
            .new_container(
                LayoutStyle::new().flex_column().width(WIDTH).height(200.0),
                &[panel],
            )
            .unwrap();
        (backdrop, panel)
    };
    assert_eq!(x_under(Direction::Ltr, build), 40.0);
    assert_eq!(x_under(Direction::Rtl, build), 40.0);
}

/// Flipping re-resolves every node, a parent possibly after its child, and the gap margin is placed from the parent's axis.
#[test]
fn a_gap_margin_lands_on_the_leading_edge_of_a_reversed_row_after_a_flip() {
    let mut engine = LayoutEngine::new();
    let first = sized(&mut engine, 50.0);
    let second = sized(&mut engine, 50.0);
    let row = engine
        .new_container(
            LayoutStyle::new().flex_row_reverse().width(WIDTH),
            &[first, second],
        )
        .unwrap();
    engine.set_leading_margin(second, true, 8.0);
    for direction in [Direction::Rtl, Direction::Ltr, Direction::Rtl] {
        engine.set_direction(direction);
        engine.mark_dirty(row).unwrap();
        lay_out(&mut engine, row);
        assert_eq!(
            engine.layout(second).unwrap().x,
            WIDTH - 50.0 - 8.0 - 50.0,
            "{direction:?}"
        );
    }
}
