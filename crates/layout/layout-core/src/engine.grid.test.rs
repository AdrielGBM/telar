use super::*;

fn cells() -> LayoutGrid {
    LayoutGrid::new(8.0, 16.0).expect("a positive, finite step")
}

fn laid_out_box(engine: &mut LayoutEngine) -> (NodeId, NodeId) {
    let probe = engine
        .new_leaf(
            LayoutStyle::new()
                .width(20.0)
                .height(20.0)
                .margin_inline_start(13.0),
        )
        .unwrap();
    let root = engine
        .new_container(LayoutStyle::row().width(200.0), &[probe])
        .unwrap();
    engine
        .compute_layout(
            root,
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(200.0),
        )
        .unwrap();
    (root, probe)
}

/// The grid belongs to the engine resolving a style rather than to the style, so one style lays out on whatever surface it lands on.
#[test]
fn the_same_style_snaps_to_the_grid_of_the_engine_it_is_laid_out_in() {
    let mut cell_engine = LayoutEngine::new();
    cell_engine.set_grid(cells());
    let mut pixel_engine = LayoutEngine::new();

    let (_, on_cells) = laid_out_box(&mut cell_engine);
    let (_, on_pixels) = laid_out_box(&mut pixel_engine);

    let cell_box = cell_engine.layout(on_cells).unwrap();
    assert_eq!((cell_box.width, cell_box.height), (24.0, 16.0));
    assert_eq!(
        cell_box.x, 8.0,
        "13px of air is under three quarters of two cells"
    );
    let pixel_box = pixel_engine.layout(on_pixels).unwrap();
    assert_eq!((pixel_box.width, pixel_box.height), (20.0, 20.0));
    assert_eq!(pixel_box.x, 13.0);
}

/// A changed grid starts again from what the style was written with: lengths an earlier grid rounded would round twice.
#[test]
fn a_new_grid_resolves_the_written_lengths_again() {
    let mut engine = LayoutEngine::new();
    engine.set_grid(cells());
    let (root, probe) = laid_out_box(&mut engine);

    assert!(engine.set_grid(LayoutGrid::UNIT));
    engine
        .compute_layout(
            root,
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(200.0),
        )
        .unwrap();

    let rect = engine.layout(probe).unwrap();
    assert_eq!((rect.width, rect.height, rect.x), (20.0, 20.0, 13.0));
    assert!(
        !engine.set_grid(LayoutGrid::UNIT),
        "an unchanged grid resolves nothing"
    );
}
