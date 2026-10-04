use super::*;

fn p(x: f32, y: f32) -> Point {
    Point::new(x, y)
}

fn dash(pattern: &[f32], offset: f32) -> Dash {
    Dash::new(pattern, offset).expect("a valid pattern")
}

/// Each dash of `path` as its points, so a test reads the result as polylines.
fn polylines(path: &PathData) -> Vec<(Vec<Point>, bool)> {
    let mut out: Vec<(Vec<Point>, bool)> = Vec::new();
    for verb in path.verbs() {
        match *verb {
            PathVerb::MoveTo(at) => out.push((vec![at], false)),
            PathVerb::LineTo(at) => out.last_mut().expect("a move first").0.push(at),
            PathVerb::Close => out.last_mut().expect("a move first").1 = true,
            PathVerb::QuadTo { .. } | PathVerb::CubicTo { .. } => {
                panic!("a split path holds straight segments only")
            }
        }
    }
    out
}

fn assert_near(actual: Point, expected: Point) {
    assert!(
        (actual.x - expected.x).abs() < 1e-3 && (actual.y - expected.y).abs() < 1e-3,
        "{actual:?} is not {expected:?}"
    );
}

fn assert_segments(actual: &[(Point, Point)], expected: &[(Point, Point)]) {
    assert_eq!(actual.len(), expected.len(), "{actual:?}");
    for (a, e) in actual.iter().zip(expected) {
        assert_near(a.0, e.0);
        assert_near(a.1, e.1);
    }
}

#[test]
fn an_even_pattern_is_kept_as_written() {
    assert_eq!(dash(&[1.0, 4.0], 0.0).lengths(), &[1.0, 4.0]);
}

#[test]
fn an_odd_pattern_is_repeated_into_an_even_one() {
    assert_eq!(
        dash(&[1.0, 2.0, 3.0], 0.0).lengths(),
        &[1.0, 2.0, 3.0, 1.0, 2.0, 3.0]
    );
}

#[test]
fn a_pattern_that_draws_no_dashes_is_none() {
    assert!(Dash::new(&[], 0.0).is_none());
    assert!(Dash::new(&[0.0, 0.0], 0.0).is_none());
    assert!(Dash::new(&[1.0, -1.0], 0.0).is_none());
    assert!(Dash::new(&[1.0, f32::NAN], 0.0).is_none());
    assert!(Dash::new(&[1.0, f32::INFINITY], 0.0).is_none());
    assert!(Dash::new(&[1.0, 4.0], f32::NAN).is_none());
}

#[test]
fn a_pattern_longer_than_the_capacity_is_none() {
    assert!(Dash::new(&[1.0; Dash::CAPACITY], 0.0).is_some());
    assert!(Dash::new(&[1.0; Dash::CAPACITY + 2], 0.0).is_none());
    assert!(Dash::new(&[1.0; Dash::CAPACITY / 2 + 1], 0.0).is_none());
}

#[test]
fn the_offset_wraps_into_one_period() {
    assert_eq!(dash(&[1.0, 4.0], 7.0).offset(), 2.0);
    assert_eq!(dash(&[1.0, 4.0], -1.0).offset(), 4.0);
    assert_eq!(dash(&[1.0, 4.0], 5.0).offset(), 0.0);
}

#[test]
fn patterns_that_draw_the_same_are_equal_and_hash_alike() {
    use std::collections::hash_map::DefaultHasher;
    let hash = |d: &Dash| {
        let mut h = DefaultHasher::new();
        d.hash(&mut h);
        h.finish()
    };
    let a = dash(&[1.0, 4.0], 0.0);
    let b = dash(&[1.0, 4.0], 5.0);
    let c = dash(&[-0.0, 4.0], -0.0);
    assert_eq!(a, b);
    assert_eq!(hash(&a), hash(&b));
    assert_eq!(c, dash(&[0.0, 4.0], 0.0));
    assert_eq!(hash(&c), hash(&dash(&[0.0, 4.0], 0.0)));
    assert_ne!(a, dash(&[1.0, 3.0], 0.0));
    assert_ne!(a, dash(&[1.0, 4.0], 1.0));
}

#[test]
fn scaling_multiplies_the_lengths_and_the_offset() {
    let scaled = dash(&[1.0, 4.0], 2.0).scaled(2.0).expect("still a pattern");
    assert_eq!(scaled.lengths(), &[2.0, 8.0]);
    assert_eq!(scaled.offset(), 4.0);
    assert!(dash(&[1.0, 4.0], 0.0).scaled(0.0).is_none());
}

#[test]
fn a_segment_is_cut_into_its_dashes() {
    let dashes = dash(&[1.0, 4.0], 0.0).split_segment(p(0.0, 0.0), p(12.0, 0.0));
    assert_segments(
        &dashes,
        &[
            (p(0.0, 0.0), p(1.0, 0.0)),
            (p(5.0, 0.0), p(6.0, 0.0)),
            (p(10.0, 0.0), p(11.0, 0.0)),
        ],
    );
}

#[test]
fn a_dash_cut_by_the_end_of_the_segment_stops_there() {
    let dashes = dash(&[3.0, 2.0], 0.0).split_segment(p(0.0, 0.0), p(0.0, 7.0));
    assert_segments(
        &dashes,
        &[(p(0.0, 0.0), p(0.0, 3.0)), (p(0.0, 5.0), p(0.0, 7.0))],
    );
}

#[test]
fn the_offset_starts_the_stroke_inside_the_pattern() {
    let dashes = dash(&[2.0, 2.0], 1.0).split_segment(p(0.0, 0.0), p(6.0, 0.0));
    assert_segments(
        &dashes,
        &[(p(0.0, 0.0), p(1.0, 0.0)), (p(3.0, 0.0), p(5.0, 0.0))],
    );
}

#[test]
fn an_offset_that_starts_in_a_gap_skips_it() {
    let dashes = dash(&[1.0, 4.0], 1.0).split_segment(p(0.0, 0.0), p(6.0, 0.0));
    assert_segments(&dashes, &[(p(4.0, 0.0), p(5.0, 0.0))]);
}

#[test]
fn a_drawn_zero_is_a_dash_with_no_length() {
    let dashes = dash(&[0.0, 4.0], 0.0).split_segment(p(0.0, 0.0), p(9.0, 0.0));
    assert_segments(
        &dashes,
        &[
            (p(0.0, 0.0), p(0.0, 0.0)),
            (p(4.0, 0.0), p(4.0, 0.0)),
            (p(8.0, 0.0), p(8.0, 0.0)),
        ],
    );
}

#[test]
fn a_dash_turns_corners_as_one_dash() {
    let path = PathData::new()
        .move_to(p(0.0, 0.0))
        .line_to(p(4.0, 0.0))
        .line_to(p(4.0, 4.0));
    let lines = polylines(&dash(&[6.0, 10.0], 0.0).split(&path, 0.1));
    assert_eq!(lines.len(), 1);
    let (points, closed) = &lines[0];
    assert!(!closed);
    assert_eq!(points.len(), 3);
    assert_near(points[0], p(0.0, 0.0));
    assert_near(points[1], p(4.0, 0.0));
    assert_near(points[2], p(4.0, 2.0));
}

#[test]
fn the_pattern_restarts_at_every_subpath() {
    let path = PathData::new()
        .move_to(p(0.0, 0.0))
        .line_to(p(3.0, 0.0))
        .move_to(p(0.0, 10.0))
        .line_to(p(3.0, 10.0));
    let lines = polylines(&dash(&[2.0, 5.0], 0.0).split(&path, 0.1));
    assert_eq!(lines.len(), 2);
    assert_near(lines[0].0[0], p(0.0, 0.0));
    assert_near(lines[0].0[1], p(2.0, 0.0));
    assert_near(lines[1].0[0], p(0.0, 10.0));
    assert_near(lines[1].0[1], p(2.0, 10.0));
}

#[test]
fn a_closed_subpath_joins_the_dash_through_its_start() {
    let square = PathData::polygon(&[p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0), p(0.0, 10.0)]);
    let lines = polylines(&dash(&[6.0, 3.0], 0.0).split(&square, 0.1));
    assert_eq!(lines.len(), 4, "{lines:?}");
    let (joined, closed) = lines.last().expect("dashes");
    assert!(!closed);
    assert_eq!(joined.len(), 3);
    assert_near(joined[0], p(0.0, 4.0));
    assert_near(joined[1], p(0.0, 0.0));
    assert_near(joined[2], p(6.0, 0.0));
}

#[test]
fn a_closed_subpath_the_pattern_never_leaves_stays_closed() {
    let square = PathData::polygon(&[p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0), p(0.0, 10.0)]);
    let lines = polylines(&dash(&[100.0, 1.0], 0.0).split(&square, 0.1));
    assert_eq!(lines.len(), 1);
    let (points, closed) = &lines[0];
    assert!(closed);
    assert_eq!(points.len(), 4);
}

#[test]
fn a_segment_after_a_close_starts_where_the_subpath_did() {
    let path = PathData::new()
        .move_to(p(0.0, 0.0))
        .line_to(p(4.0, 0.0))
        .close()
        .line_to(p(0.0, 4.0));
    let lines = polylines(&dash(&[100.0, 1.0], 0.0).split(&path, 0.1));
    let last = &lines.last().expect("dashes").0;
    assert_near(last[0], p(0.0, 0.0));
    assert_near(last[1], p(0.0, 4.0));
}

#[test]
fn curves_are_flattened_before_they_are_cut() {
    let arch = PathData::new()
        .move_to(p(0.0, 0.0))
        .quad_to(p(50.0, 50.0), p(100.0, 0.0));
    let drawn: f32 = polylines(&dash(&[5.0, 5.0], 0.0).split(&arch, 0.05))
        .iter()
        .map(|(points, _)| points.windows(2).map(|w| distance(w[0], w[1])).sum::<f32>())
        .sum();
    let whole: f32 = polylines(&dash(&[1000.0, 1.0], 0.0).split(&arch, 0.05))
        .iter()
        .map(|(points, _)| points.windows(2).map(|w| distance(w[0], w[1])).sum::<f32>())
        .sum();
    assert!(whole > 100.0, "an arch is longer than its chord: {whole}");
    assert!(
        (drawn - whole / 2.0).abs() < 5.0,
        "half of {whole} drawn, got {drawn}"
    );
}

#[test]
fn a_flattened_curve_stays_within_the_tolerance() {
    let (cx, cy, r) = (0.0f32, 0.0f32, 100.0f32);
    let k = crate::BEZIER_CIRCLE_K;
    let quarter = PathData::new().move_to(p(cx + r, cy)).cubic_to(
        p(cx + r, cy + k * r),
        p(cx + k * r, cy + r),
        p(cx, cy + r),
    );
    let lines = polylines(&dash(&[1000.0, 1.0], 0.0).split(&quarter, 0.1));
    let points = &lines[0].0;
    assert!(
        points.len() > 8,
        "a quarter circle is more than a few lines"
    );
    for w in points.windows(2) {
        let mid = p((w[0].x + w[1].x) / 2.0, (w[0].y + w[1].y) / 2.0);
        let off = (mid.x.hypot(mid.y) - r).abs();
        assert!(off < 0.2, "chord midpoint {off} off the circle");
    }
}

#[test]
fn an_empty_path_has_no_dashes() {
    assert!(
        dash(&[1.0, 1.0], 0.0)
            .split(&PathData::new(), 0.1)
            .verbs()
            .is_empty()
    );
}
