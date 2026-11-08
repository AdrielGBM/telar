use super::*;

#[test]
fn edges_round_so_neighbours_touch() {
    let cell = CellSize::default();
    let first = (cell.col_at(0.0), cell.col_at(37.5));
    let second = (cell.col_at(37.5), cell.col_at(75.0));
    assert_eq!(first.1, second.0, "a shared edge must land on one column");
}

#[test]
fn the_default_cell_is_twice_as_tall_as_wide() {
    let cell = CellSize::default();
    assert_eq!(cell.height, 2.0 * cell.width);
}
