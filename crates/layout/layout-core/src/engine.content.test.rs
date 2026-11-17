use super::*;
use crate::style::{AlignItems, SizeDimension};
use crate::track::TemplateTrack;

/// A line of text 500 wide that wraps onto a second line in any narrower width.
fn sentence(engine: &mut LayoutEngine, style: LayoutStyle) -> NodeId {
    engine
        .new_measured_leaf(
            style,
            Box::new(|input: MeasureInput| match input.width {
                AvailableSpace::Definite(w) if w < 500.0 => (w, 40.0),
                AvailableSpace::MinContent => (250.0, 40.0),
                _ => (500.0, 20.0),
            }),
        )
        .unwrap()
}

fn laid_out(engine: &mut LayoutEngine, parent: NodeId) {
    engine
        .compute_layout(
            parent,
            AvailableSpace::Definite(300.0),
            AvailableSpace::Definite(600.0),
        )
        .unwrap();
}

/// A box the content's size with an aspect ratio of 2, around a picture 200 by 100 that fills it once it has a size: an image, as a component draws one.
fn picture_box(engine: &mut LayoutEngine, place: impl Fn(LayoutStyle) -> LayoutStyle) -> NodeId {
    let fill = SizeDimension::Percent(1.0);
    let picture = engine
        .new_leaf(
            LayoutStyle::new()
                .width(200.0)
                .height(100.0)
                .min_width(fill)
                .min_height(fill)
                .max_width(fill)
                .max_height(fill),
        )
        .unwrap();
    engine
        .new_container(
            place(
                LayoutStyle::row()
                    .flex_shrink(0.0)
                    .width(SizeDimension::Content)
                    .height(SizeDimension::Content)
                    .aspect_ratio(2.0),
            ),
            &[picture],
        )
        .unwrap()
}

fn size(engine: &LayoutEngine, node: NodeId) -> (f32, f32) {
    let rect = engine.layout(node).unwrap();
    (rect.width, rect.height)
}

#[test]
fn a_content_width_is_the_content_unwrapped_however_narrow_the_parent() {
    let mut engine = LayoutEngine::new();
    let text = sentence(
        &mut engine,
        LayoutStyle::new().width(SizeDimension::Content),
    );
    let column = engine
        .new_container(LayoutStyle::column().width(300.0), &[text])
        .unwrap();
    laid_out(&mut engine, column);

    assert_eq!(
        size(&engine, text),
        (500.0, 20.0),
        "one line, measured at its own width rather than the column's"
    );
}

#[test]
fn a_stretching_parent_leaves_a_content_cross_size_at_the_start() {
    let mut engine = LayoutEngine::new();
    let text = sentence(
        &mut engine,
        LayoutStyle::new().height(SizeDimension::Content),
    );
    let row = engine
        .new_container(LayoutStyle::row().width(800.0).height(200.0), &[text])
        .unwrap();
    laid_out(&mut engine, row);

    let text_box = engine.layout(text).unwrap();
    assert_eq!(
        (text_box.y, text_box.height),
        (0.0, 20.0),
        "not stretched to the row's height"
    );
}

#[test]
fn a_content_cross_size_keeps_the_alignment_its_parent_asks_for() {
    let mut engine = LayoutEngine::new();
    let text = sentence(
        &mut engine,
        LayoutStyle::new().width(SizeDimension::Content),
    );
    let column = engine
        .new_container(
            LayoutStyle::column()
                .width(700.0)
                .align_items(AlignItems::CENTER),
            &[text],
        )
        .unwrap();
    laid_out(&mut engine, column);

    let text_box = engine.layout(text).unwrap();
    assert_eq!((text_box.x, text_box.width), (100.0, 500.0));
}

#[test]
fn a_parent_that_turns_its_axis_stretches_what_is_now_an_auto_cross_size() {
    let mut engine = LayoutEngine::new();
    let text = sentence(
        &mut engine,
        LayoutStyle::new().width(SizeDimension::Content),
    );
    let parent = engine
        .new_container(LayoutStyle::column().width(800.0).height(200.0), &[text])
        .unwrap();
    laid_out(&mut engine, parent);
    assert_eq!(size(&engine, text), (500.0, 20.0));

    engine
        .set_style(parent, LayoutStyle::row().width(800.0).height(200.0))
        .unwrap();
    laid_out(&mut engine, parent);

    assert_eq!(
        size(&engine, text),
        (500.0, 200.0),
        "a row stretches the auto height of what it holds"
    );
}

#[test]
fn a_box_handed_to_another_parent_aligns_as_that_parent_asks() {
    let mut engine = LayoutEngine::new();
    let image = picture_box(&mut engine, |style| style.height(SizeDimension::Auto));
    let set = engine
        .new_container(LayoutStyle::column().width(800.0), &[])
        .unwrap();
    let added = engine
        .new_container(LayoutStyle::row().width(800.0).height(300.0), &[])
        .unwrap();
    let root = engine
        .new_container(LayoutStyle::column().width(800.0), &[set, added])
        .unwrap();
    engine.set_children(set, &[image]).unwrap();
    laid_out(&mut engine, root);
    assert_eq!(size(&engine, image), (200.0, 100.0));

    engine.add_child(added, image).unwrap();
    laid_out(&mut engine, root);
    assert_eq!(
        size(&engine, image),
        (600.0, 300.0),
        "a row stretches its auto height, which the ratio carries to the width"
    );
}

#[test]
fn a_content_sized_box_with_an_aspect_ratio_is_its_content_in_a_stretching_column() {
    let mut engine = LayoutEngine::new();
    let image = picture_box(&mut engine, |style| style);
    let column = engine
        .new_container(LayoutStyle::column().width(300.0), &[image])
        .unwrap();
    laid_out(&mut engine, column);

    assert_eq!(size(&engine, image), (200.0, 100.0));
}

#[test]
fn a_content_sized_box_with_an_aspect_ratio_is_its_content_in_a_stretching_row() {
    let mut engine = LayoutEngine::new();
    let image = picture_box(&mut engine, |style| style);
    let row = engine
        .new_container(LayoutStyle::row().width(300.0).height(600.0), &[image])
        .unwrap();
    laid_out(&mut engine, row);

    assert_eq!(size(&engine, image), (200.0, 100.0));
}

#[test]
fn a_set_height_carries_through_the_ratio_to_a_content_width() {
    for parent in [
        LayoutStyle::column(),
        LayoutStyle::row(),
        LayoutStyle::new(),
    ] {
        let mut engine = LayoutEngine::new();
        let image = picture_box(&mut engine, |style| style.height(50.0));
        let parent = engine
            .new_container(parent.width(300.0).height(600.0), &[image])
            .unwrap();
        laid_out(&mut engine, parent);

        assert_eq!(size(&engine, image), (100.0, 50.0));
    }
}

#[test]
fn a_set_width_carries_through_the_ratio_to_a_content_height() {
    for parent in [
        LayoutStyle::column(),
        LayoutStyle::row(),
        LayoutStyle::new(),
    ] {
        let mut engine = LayoutEngine::new();
        let image = picture_box(&mut engine, |style| style.width(400.0));
        let parent = engine
            .new_container(parent.width(300.0).height(600.0), &[image])
            .unwrap();
        laid_out(&mut engine, parent);

        assert_eq!(size(&engine, image), (400.0, 200.0));
    }
}

#[test]
fn a_content_flex_basis_starts_an_item_at_its_content_unwrapped() {
    let mut engine = LayoutEngine::new();
    let text = sentence(
        &mut engine,
        LayoutStyle::new()
            .flex_basis(SizeDimension::Content)
            .flex_shrink(0.0),
    );
    let row = engine
        .new_container(LayoutStyle::row().width(800.0), &[text])
        .unwrap();
    laid_out(&mut engine, row);

    assert_eq!(size(&engine, text).0, 500.0);
}

#[test]
fn a_content_grid_track_is_as_wide_as_what_it_holds_unwrapped() {
    let mut engine = LayoutEngine::new();
    let text = sentence(&mut engine, LayoutStyle::new());
    let grid = engine
        .new_container(
            LayoutStyle::new()
                .display_grid()
                .width(800.0)
                .grid_template_columns(vec![
                    TemplateTrack::length(SizeDimension::Content),
                    TemplateTrack::fr(1.0),
                ]),
            &[text],
        )
        .unwrap();
    laid_out(&mut engine, grid);

    assert_eq!(size(&engine, text).0, 500.0);
}
