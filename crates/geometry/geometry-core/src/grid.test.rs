use super::*;

/// The property the whole module exists for: on a grid, a snapped height covers the same number of steps wherever it is put.
#[test]
fn a_snapped_size_covers_the_same_steps_at_every_position() {
    let g = LayoutGrid::new(8.0, 16.0);
    let h = g.snap_size_y(37.0);
    let rows = |y: f32| ((y + h) / 16.0).round() - (y / 16.0).round();
    let at_zero = rows(0.0);
    for step in 0..64 {
        let y = step as f32 * 2.5;
        assert_eq!(rows(y), at_zero, "height {h} changed size at y={y}");
    }
}

/// And the counter-example that motivates it: the same box unsnapped does not.
#[test]
fn an_unsnapped_size_does_not() {
    let rows = |y: f32| ((y + 37.0) / 16.0).round() - (y / 16.0).round();
    assert_ne!(
        rows(0.0),
        rows(20.0),
        "37px at a 16px step should be the unstable case"
    );
}

#[test]
fn a_unit_grid_changes_nothing() {
    let g = LayoutGrid::UNIT;
    assert!(g.is_unit(), "a unit grid snaps nothing: {g:?}");
    for v in [0.0, 0.5, 1.0, 10.5, 37.0, -4.25] {
        assert_eq!(g.snap_size_y(v), v);
        assert_eq!(g.snap_space_y(v), v);
        assert_eq!(g.snap_pos_y(v), v);
    }
}

/// A box the author asked for may not be quantised out of existence, but air may.
#[test]
fn a_size_keeps_a_step_where_space_rounds_away() {
    let g = LayoutGrid::new(8.0, 16.0);
    assert_eq!(g.snap_size_y(2.0), 16.0);
    assert_eq!(g.snap_space_y(2.0), 0.0);
    // And a size stays at the nearest step rather than following the space bias.
    assert_eq!(g.snap_size_y(26.0), 32.0);
    assert_eq!(g.snap_space_y(26.0), 16.0);
    assert_eq!(g.snap_size_y(0.0), 0.0);
}

/// The threshold, at the two values that define it.
#[test]
fn space_is_kept_only_when_it_nearly_fills_a_step() {
    let g = LayoutGrid::new(8.0, 16.0);
    assert_eq!(g.snap_space_y(11.0), 0.0, "11 of 16 is not worth a row");
    assert_eq!(g.snap_space_y(12.0), 16.0, "12 of 16 is");
    // A button's padding under the sandbox's own theme: half a row of air, which the terminal spends nothing on.
    assert_eq!(g.snap_space_y(10.5), 0.0);
    // The bias is a fraction of the step, so the narrower axis follows the same rule.
    assert_eq!(g.snap_space_x(24.5), 24.0);
    assert_eq!(g.snap_space_x(5.0), 0.0);
    assert_eq!(g.snap_space_x(6.0), 8.0);
}

/// The property the rounding mode exists for: a whole number of steps stays a whole number of steps, including across the origin — which is where a box scrolled above the viewport sits.
#[test]
fn a_whole_size_survives_being_translated_past_zero() {
    let g = LayoutGrid::new(8.0, 16.0);
    let h = 896.0;
    let rows = |y: f32| ((g.snap_pos_y(y + h) - g.snap_pos_y(y)) / 16.0) as i32;
    for step in -40..40 {
        let y = step as f32 * 2.5;
        assert_eq!(
            rows(y),
            56,
            "a 56-cell box measured {} cells at y={y}",
            rows(y)
        );
    }
}

/// A negative coordinate is what a scrolled-away origin looks like.
#[test]
fn a_position_snaps_in_both_directions() {
    let g = LayoutGrid::new(8.0, 16.0);
    assert_eq!(g.snap_pos_y(-20.0), -16.0);
    assert_eq!(g.snap_pos_y(20.0), 16.0);
}
