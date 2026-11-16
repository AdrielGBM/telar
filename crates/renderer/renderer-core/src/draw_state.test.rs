use super::*;

// Guards the push_matrix compose argument order (cumulative ∘ matrix) against the pre-refactor hand-rolled `compose_matrix(parent, child)`; a swapped order silently corrupts nested transforms.
#[test]
fn push_matrix_matches_legacy_compose_order() {
    let cumulative = Transform {
        a: 2.0,
        b: 0.1,
        c: -0.2,
        d: 2.0,
        e: 10.0,
        f: 20.0,
    };
    let matrix = Transform {
        a: 1.0,
        b: 0.5,
        c: -0.5,
        d: 1.0,
        e: 5.0,
        f: 7.0,
    };

    let Transform {
        a: a1,
        b: b1,
        c: c1,
        d: d1,
        e: e1,
        f: f1,
    } = matrix;
    let Transform {
        a: a2,
        b: b2,
        c: c2,
        d: d2,
        e: e2,
        f: f2,
    } = cumulative;
    let expected = Transform {
        a: a2 * a1 + c2 * b1,
        b: b2 * a1 + d2 * b1,
        c: a2 * c1 + c2 * d1,
        d: b2 * c1 + d2 * d1,
        e: a2 * e1 + c2 * f1 + e2,
        f: b2 * e1 + d2 * f1 + f2,
    };

    let mut state = DrawState::new();
    state.push_matrix(cumulative);
    state.push_matrix(matrix);
    assert_eq!(state.cumulative_matrix, expected);
}

/// A clip nested inside one it does not touch must clip away to nothing. The failing case is a widget that emits its own clip — an image with a `Cover` overflow or a corner radius — scrolled out of the viewport containing it: falling back to the child's own rect scissors to a box outside the parent, and the widget goes on being drawn there, over whatever the panel happens to be sitting on.
#[test]
fn a_clip_outside_its_parent_clips_away_to_nothing() {
    let viewport = Rect::new(0.0, 0.0, 340.0, 300.0);
    let mut state = DrawState::new();
    assert_eq!(
        state.push_clip(viewport),
        viewport,
        "the outermost clip is itself"
    );

    let escaped = state.push_clip(Rect::new(0.0, 560.0, 56.0, 56.0));
    assert_eq!(
        (escaped.width, escaped.height),
        (0.0, 0.0),
        "an image scrolled out of its panel must not be drawn at all, got {escaped:?}"
    );
    assert!(
        viewport
            .intersect(Rect::new(escaped.x, escaped.y, 1.0, 1.0))
            .is_some(),
        "and the empty clip sits inside the parent, so a backend that rounds it up to 1×1 draws that \
         pixel where it was already allowed to: {escaped:?}"
    );

    state.pop_clip();
    let visible = state.push_clip(Rect::new(0.0, 280.0, 56.0, 56.0));
    assert_eq!(visible, Rect::new(0.0, 280.0, 56.0, 20.0));

    let mut fresh = DrawState::new();
    let alone = Rect::new(12.0, 34.0, 56.0, 78.0);
    assert_eq!(fresh.push_clip(alone), alone);
}

/// What text is laid out at. The rect a matrix scales already grows; without one number saying by how much, the letters inside it would not — which is a box of the right size holding text of the wrong one.
#[test]
fn the_scale_of_a_matrix_is_what_it_makes_things() {
    let mut state = DrawState::new();
    assert_eq!(
        state.cumulative_matrix.scale(),
        1.0,
        "nothing scales nothing"
    );

    state.push_matrix(Transform::scale_around(2.0, 2.0, 0.0, 0.0));
    assert_eq!(state.cumulative_matrix.scale(), 2.0);

    state.push_matrix(Transform::scale_around(3.0, 3.0, 0.0, 0.0));
    assert_eq!(state.cumulative_matrix.scale(), 6.0);

    state.pop_matrix();
    assert_eq!(state.cumulative_matrix.scale(), 2.0);
}
