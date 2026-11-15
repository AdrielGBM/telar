use super::*;
use crate::{AvailableSpace, LayoutStyle, compute_layout, new_leaf};

fn cells() -> LayoutGrid {
    LayoutGrid::new(8.0, 16.0).expect("a positive, finite step")
}

fn laid_out_size() -> (f32, f32) {
    let (node, rect) = new_leaf(LayoutStyle::new().width(20.0).height(20.0)).unwrap();
    compute_layout(
        node,
        AvailableSpace::Definite(100.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();
    let rect = rect.peek();
    (rect.width, rect.height)
}

#[test]
fn a_tree_is_laid_out_on_the_grid_its_surface_declares() {
    assert_eq!(laid_out_size(), (20.0, 20.0));
    set_layout_grid(cells());
    assert_eq!(laid_out_size(), (24.0, 16.0));
}

#[test]
fn a_world_keeps_the_grid_it_was_given() {
    let cell_world = LayoutGridContext::new();
    cell_world.with_entered(|| set_layout_grid(cells()));
    assert_eq!(layout_grid(), LayoutGrid::UNIT);
    assert_eq!(cell_world.with_entered(layout_grid), cells());
}
