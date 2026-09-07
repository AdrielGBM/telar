use super::*;
use crate::{BorderRadius, DrawCommand, style::RectStyle};
use geometry_core::Rect;
use std::sync::Arc;

fn rect_cmd(x: f32, y: f32, w: f32, h: f32) -> DrawCommand {
    DrawCommand::Rect {
        rect: Rect::new(x, y, w, h),
        style: Arc::new(RectStyle::default()),
    }
}

#[test]
fn compute_dirty_rect_len_mismatch_returns_none() {
    let a = vec![rect_cmd(0.0, 0.0, 10.0, 10.0)];
    let b = vec![];
    assert!(
        compute_dirty_rect(&a, &b, |cmd, m| culling::command_visual_rect(
            cmd,
            m,
            &FontMetrics::default()
        ))
        .is_none(),
        "a shorter list is not a bounded change"
    );
}

// Regression: an opacity-only layer change must force a full re-render. The layer has no geometry and its inner commands compare equal, so treating it as a normal change yields an empty dirty list.
#[test]
fn changed_push_layer_opacity_forces_full_render() {
    let inner = rect_cmd(10.0, 10.0, 50.0, 50.0);
    let old = vec![
        DrawCommand::PushLayer {
            opacity: 0.9,
            backdrop_blur: 0.0,
        },
        inner.clone(),
        DrawCommand::PopLayer,
    ];
    let new = vec![
        DrawCommand::PushLayer {
            opacity: 0.8,
            backdrop_blur: 0.0,
        },
        inner,
        DrawCommand::PopLayer,
    ];
    assert!(
        compute_dirty_rect(&new, &old, |cmd, m| culling::command_visual_rect(
            cmd,
            m,
            &FontMetrics::default()
        ))
        .is_none(),
        "changed layer must not be expressible as a bounded dirty region"
    );
}

#[test]
fn compute_dirty_rect_no_change_returns_none() {
    let a = vec![rect_cmd(0.0, 0.0, 10.0, 10.0)];
    assert!(
        compute_dirty_rect(&a, &a, |cmd, m| culling::command_visual_rect(
            cmd,
            m,
            &FontMetrics::default()
        ))
        .is_none(),
        "an unchanged list dirties nothing"
    );
}

#[test]
fn compute_dirty_rect_single_change() {
    let old = vec![rect_cmd(0.0, 0.0, 10.0, 10.0)];
    let new = vec![rect_cmd(5.0, 0.0, 10.0, 10.0)];
    let rects = compute_dirty_rect(&new, &old, |cmd, m| {
        culling::command_visual_rect(cmd, m, &FontMetrics::default())
    })
    .unwrap();
    let dirty = rects.iter().copied().reduce(Rect::union).unwrap();
    assert!(
        dirty.x <= 0.0,
        "the dirty rect must reach the left edge of the change: {dirty:?}"
    );
    assert!(
        dirty.x + dirty.width >= 15.0,
        "and its right edge: {dirty:?}"
    );
}

#[test]
fn compute_dirty_rect_disjoint_changes_stay_separate() {
    let old = vec![
        rect_cmd(0.0, 0.0, 10.0, 10.0),
        rect_cmd(500.0, 500.0, 10.0, 10.0),
    ];
    let new = vec![
        rect_cmd(0.0, 0.0, 20.0, 20.0),
        rect_cmd(500.0, 500.0, 20.0, 20.0),
    ];
    let rects = compute_dirty_rect(&new, &old, |cmd, m| {
        culling::command_visual_rect(cmd, m, &FontMetrics::default())
    })
    .unwrap();
    assert_eq!(rects.len(), 2);
    for r in &rects {
        assert!(
            r.width < 100.0 && r.height < 100.0,
            "two far-apart changes must stay two tight rects, not one spanning box: {r:?}"
        );
    }
}

#[test]
fn compute_dirty_rect_translate_shift() {
    let old = vec![
        DrawCommand::PushMatrix {
            matrix: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        },
        rect_cmd(0.0, 0.0, 10.0, 10.0),
        DrawCommand::PopMatrix,
    ];
    let new = vec![
        DrawCommand::PushMatrix {
            matrix: [1.0, 0.0, 0.0, 1.0, 5.0, 5.0],
        },
        rect_cmd(0.0, 0.0, 10.0, 10.0),
        DrawCommand::PopMatrix,
    ];
    let rects = compute_dirty_rect(&new, &old, |cmd, m| {
        culling::command_visual_rect(cmd, m, &FontMetrics::default())
    })
    .unwrap();
    let dirty = rects.iter().copied().reduce(Rect::union).unwrap();
    assert!(
        dirty.x <= 0.0,
        "a translated command dirties where it was: {dirty:?}"
    );
    assert!(dirty.y <= 0.0, "on both axes: {dirty:?}");
    assert!(
        dirty.x + dirty.width >= 15.0,
        "and where it went: {dirty:?}"
    );
    assert!(dirty.y + dirty.height >= 15.0, "on both axes: {dirty:?}");
}

#[test]
fn compute_dirty_rect_clip_change_returns_none() {
    // A changed PushClip must force a full re-render; elements inside the old/new clip boundary can't be expressed as a bounded dirty rect.
    let old = vec![
        DrawCommand::PushClip {
            rect: Rect::new(0.0, 0.0, 100.0, 600.0),
            radius: BorderRadius::zero(),
        },
        rect_cmd(0.0, 100.0, 100.0, 20.0),
        DrawCommand::PopClip,
    ];
    let new = vec![
        DrawCommand::PushClip {
            rect: Rect::new(0.0, 0.0, 100.0, 400.0),
            radius: BorderRadius::zero(),
        },
        rect_cmd(0.0, 100.0, 100.0, 20.0),
        DrawCommand::PopClip,
    ];
    assert!(
        compute_dirty_rect(&new, &old, |cmd, m| culling::command_visual_rect(
            cmd,
            m,
            &FontMetrics::default()
        ))
        .is_none(),
        "a changed clip cannot be expressed as a bounded dirty region"
    );
}

#[test]
fn detect_scroll_blit_no_change_returns_none() {
    let cmds = vec![
        DrawCommand::PushClip {
            rect: Rect::new(0.0, 0.0, 100.0, 200.0),
            radius: BorderRadius::zero(),
        },
        DrawCommand::PushMatrix {
            matrix: [1.0, 0.0, 0.0, 1.0, 0.0, -50.0],
        },
        rect_cmd(0.0, 0.0, 100.0, 400.0),
        DrawCommand::PopMatrix,
        DrawCommand::PopClip,
    ];
    assert!(
        detect_scroll_blit(&cmds, &cmds).is_none(),
        "an unchanged list scrolled by nothing"
    );
}

#[test]
fn detect_scroll_blit_repaints_static_visual_before_scroll() {
    // A static element before the scroll transform lives inside its clip, so the blit shifts its pixels and it is repainted with its ghost rather than bailing to a full re-render.
    let old = vec![
        DrawCommand::PushClip {
            rect: Rect::new(0.0, 0.0, 100.0, 200.0),
            radius: BorderRadius::zero(),
        },
        rect_cmd(0.0, 0.0, 100.0, 30.0), // header — before scroll
        DrawCommand::PushMatrix {
            matrix: [1.0, 0.0, 0.0, 1.0, 0.0, -50.0],
        },
        rect_cmd(0.0, 0.0, 100.0, 400.0),
        DrawCommand::PopMatrix,
        DrawCommand::PopClip,
    ];
    let new = vec![
        DrawCommand::PushClip {
            rect: Rect::new(0.0, 0.0, 100.0, 200.0),
            radius: BorderRadius::zero(),
        },
        rect_cmd(0.0, 0.0, 100.0, 30.0), // unchanged header
        DrawCommand::PushMatrix {
            matrix: [1.0, 0.0, 0.0, 1.0, 0.0, -60.0],
        }, // scrolled
        rect_cmd(0.0, 0.0, 100.0, 400.0),
        DrawCommand::PopMatrix,
        DrawCommand::PopClip,
    ];
    let sb = detect_scroll_blit(&new, &old).expect("blit should apply with a static header");
    let covers_header = sb
        .extra_dirty
        .iter()
        .any(|r| r.x <= 50.0 && r.x + r.width >= 50.0 && r.y <= 15.0 && r.y + r.height >= 15.0);
    assert!(covers_header, "header not repainted: {:?}", sb.extra_dirty);
}

#[test]
fn detect_scroll_blit_repaints_static_visual_after_scroll() {
    let old = vec![
        DrawCommand::PushClip {
            rect: Rect::new(0.0, 0.0, 100.0, 200.0),
            radius: BorderRadius::zero(),
        },
        DrawCommand::PushMatrix {
            matrix: [1.0, 0.0, 0.0, 1.0, 0.0, -50.0],
        },
        rect_cmd(0.0, 0.0, 100.0, 400.0),
        DrawCommand::PopMatrix,
        rect_cmd(0.0, 170.0, 100.0, 30.0), // footer — after scroll, unchanged
        DrawCommand::PopClip,
    ];
    let new = vec![
        DrawCommand::PushClip {
            rect: Rect::new(0.0, 0.0, 100.0, 200.0),
            radius: BorderRadius::zero(),
        },
        DrawCommand::PushMatrix {
            matrix: [1.0, 0.0, 0.0, 1.0, 0.0, -60.0],
        }, // scrolled
        rect_cmd(0.0, 0.0, 100.0, 400.0),
        DrawCommand::PopMatrix,
        rect_cmd(0.0, 170.0, 100.0, 30.0), // footer unchanged
        DrawCommand::PopClip,
    ];
    let sb = detect_scroll_blit(&new, &old).expect("blit should apply with a static footer");
    let covers_footer = sb
        .extra_dirty
        .iter()
        .any(|r| r.x <= 50.0 && r.x + r.width >= 50.0 && r.y <= 185.0 && r.y + r.height >= 185.0);
    assert!(covers_footer, "footer not repainted: {:?}", sb.extra_dirty);
}

#[test]
fn compute_dirty_rect_nested_matrix_position_change() {
    // Two nested PushMatrix levels: a translation change in the OUTER matrix must dirty the inner rect at both its old and new composed positions. Exercises the inline DrawState's cumulative-matrix composition (the former two-Vec walk).
    let make = |outer_ty: f32| {
        vec![
            DrawCommand::PushMatrix {
                matrix: [1.0, 0.0, 0.0, 1.0, 0.0, outer_ty],
            },
            DrawCommand::PushMatrix {
                matrix: [1.0, 0.0, 0.0, 1.0, 10.0, 10.0],
            },
            rect_cmd(0.0, 0.0, 10.0, 10.0),
            DrawCommand::PopMatrix,
            DrawCommand::PopMatrix,
        ]
    };
    let old = make(0.0);
    let new = make(50.0);
    let rects = compute_dirty_rect(&new, &old, |cmd, m| {
        culling::command_visual_rect(cmd, m, &FontMetrics::default())
    })
    .unwrap();
    let dirty = rects.iter().copied().reduce(Rect::union).unwrap();
    assert!(
        dirty.y <= 10.0,
        "a change under a nested matrix dirties from its top: {dirty:?}"
    );
    assert!(
        dirty.y + dirty.height >= 70.0,
        "down to its bottom: {dirty:?}"
    );
}

#[test]
fn detect_scroll_blit_pure_y_scroll() {
    let old = vec![
        DrawCommand::PushClip {
            rect: Rect::new(0.0, 0.0, 100.0, 200.0),
            radius: BorderRadius::zero(),
        },
        DrawCommand::PushMatrix {
            matrix: [1.0, 0.0, 0.0, 1.0, 0.0, -50.0],
        },
        rect_cmd(0.0, 0.0, 100.0, 400.0),
        DrawCommand::PopMatrix,
        DrawCommand::PopClip,
    ];
    let new = vec![
        DrawCommand::PushClip {
            rect: Rect::new(0.0, 0.0, 100.0, 200.0),
            radius: BorderRadius::zero(),
        },
        DrawCommand::PushMatrix {
            matrix: [1.0, 0.0, 0.0, 1.0, 0.0, -60.0],
        },
        rect_cmd(0.0, 0.0, 100.0, 400.0),
        DrawCommand::PopMatrix,
        DrawCommand::PopClip,
    ];
    let blit = detect_scroll_blit(&new, &old).unwrap();
    assert_eq!(blit.delta_y, -10);
    assert_eq!(blit.exposed_band.y, 190.0);
    assert_eq!(blit.exposed_band.height, 10.0);
}
