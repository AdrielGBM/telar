use geometry_core::Rect;
use smallvec::{SmallVec, smallvec};

use crate::{
    DrawCommand, culling,
    culling::FontMetrics,
    draw_state::{DrawState, IDENTITY_MATRIX},
};

// Advances a DrawState's cumulative matrix by a single command, mirroring for_each_with_matrix: PushMatrix/PopMatrix update the matrix first and every command then reads state.cumulative_matrix.
fn advance_matrix(state: &mut DrawState, cmd: &DrawCommand) {
    match cmd {
        DrawCommand::PushMatrix { matrix } => state.push_matrix(*matrix),
        DrawCommand::PopMatrix => state.pop_matrix(),
        _ => {}
    }
}

/// Inline capacity for the dirty-rect list. Beyond this the rects are collapsed into a single union (see MAX_DIRTY_RECTS).
pub type DirtyRects = SmallVec<[Rect; 8]>;

/// Above this count we stop tracking individual disjoint regions and fall back to a single union rect, keeping the per-frame work bounded.
const MAX_DIRTY_RECTS: usize = 4;

/// Two rects that touch or overlap (within `slop` pixels) should be merged so the dirty list stays small and the skip test stays cheap.
fn rects_adjacent_or_overlapping(a: Rect, b: Rect, slop: f32) -> bool {
    a.x <= b.x + b.width + slop
        && b.x <= a.x + a.width + slop
        && a.y <= b.y + b.height + slop
        && b.y <= a.y + a.height + slop
}

// Merges `r` into the accumulated dirty list. If `r` is adjacent to or overlaps an existing rect, the two are unioned (which can cascade-merge further); otherwise `r` is added separately. Once the list would exceed MAX_DIRTY_RECTS distinct regions it collapses to a single union to bound growth.
fn push_dirty_rect(rects: &mut DirtyRects, r: Rect) {
    // Merge slop in pixels: regions separated by a thin gap are cheaper to repaint as one than to track separately.
    const SLOP: f32 = 1.0;
    if let Some(idx) = rects
        .iter()
        .position(|e| rects_adjacent_or_overlapping(*e, r, SLOP))
    {
        let mut merged = rects[idx].union(r);
        rects.swap_remove(idx);
        // The merged rect may now touch other entries; keep folding until it is disjoint from all of them.
        let mut i = 0;
        while i < rects.len() {
            if rects_adjacent_or_overlapping(rects[i], merged, SLOP) {
                merged = rects[i].union(merged);
                rects.swap_remove(i);
            } else {
                i += 1;
            }
        }
        rects.push(merged);
    } else {
        rects.push(r);
    }

    if rects.len() > MAX_DIRTY_RECTS {
        let union = rects
            .iter()
            .copied()
            .reduce(Rect::union)
            .expect("non-empty");
        *rects = smallvec![union];
    }
}

/// When a pure axis-aligned scroll is detected, this describes what changed.
pub struct ScrollBlit {
    /// The clipping rect that encloses the scrollable content.
    pub scroll_clip: Rect,
    /// Horizontal pixel shift (negative = content moved left = scroll right). Zero for Y-only scrolls.
    pub delta_x: i32,
    /// Vertical pixel shift (negative = content moved up = scroll down). Zero for X-only scrolls.
    pub delta_y: i32,
    /// The strip of newly exposed pixels that must be re-rendered (horizontal band for Y scrolls, vertical band for X scrolls).
    pub exposed_band: Rect,
    /// Regions outside the scrolled content that the blit displaced and must be repainted in place: changed overlays (e.g. the scrollbar) and any static element drawn before/after the scroll block (fixed headers/footers, dev overlays). Each entry already unions the element with the "ghost" position the blit shifted its pixels to.
    pub extra_dirty: SmallVec<[Rect; 8]>,
}

/// Beyond this many displaced regions the fixed UI is complex enough that a full re-render is simpler (and likely cheaper) than tracking them all; `detect_scroll_blit` bails to `None`.
const MAX_SCROLL_EXTRA_DIRTY: usize = 8;

fn matrix_as_translation(m: &[f32; 6]) -> Option<(f32, f32)> {
    if m[0] == 1.0 && m[1] == 0.0 && m[2] == 0.0 && m[3] == 1.0 {
        Some((m[4], m[5]))
    } else {
        None
    }
}

// The region to repaint for an element the scroll blit displaced: its current position (`new_r`) unioned with the "ghost" — its previous pixels shifted by the blit delta (`old_r` translated by (dx, dy)). Repainting it redraws the element at rest and the scrolled content the ghost overlaps. Returns None when the element has no visual footprint in either frame.
fn displaced_region(new_r: Option<Rect>, old_r: Option<Rect>, dx: f32, dy: f32) -> Option<Rect> {
    let ghost = old_r.map(|r| Rect::new(r.x + dx, r.y + dy, r.width, r.height));
    match (new_r, ghost) {
        (Some(a), Some(b)) => Some(a.union(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

/// Compare two consecutive DrawCommand slices and return the list of disjoint regions that changed visually; returns None if a full re-render is required. A `Some(vec)` where vec is non-empty enumerates the changed regions so the caller can skip a command only when it overlaps none of them.
pub fn compute_dirty_rect(
    new_cmds: &[DrawCommand],
    old_cmds: &[DrawCommand],
    visual_rect: impl Fn(&DrawCommand, [f32; 6]) -> Option<Rect>,
) -> Option<DirtyRects> {
    if new_cmds.len() != old_cmds.len() {
        return None;
    }

    let mut dirty: DirtyRects = SmallVec::new();
    // Advance one cumulative matrix per slice inline instead of materializing two full Vecs: cumulative_matrix at command i is identical to the old matrices[i] by construction.
    let mut new_state = DrawState::new();
    let mut old_state = DrawState::new();

    for (new_cmd, old_cmd) in new_cmds.iter().zip(old_cmds.iter()) {
        advance_matrix(&mut new_state, new_cmd);
        advance_matrix(&mut old_state, old_cmd);
        let new_matrix = new_state.cumulative_matrix;
        let old_matrix = old_state.cumulative_matrix;

        if new_cmd != old_cmd {
            // A changed clip boundary cannot be expressed as a bounded dirty rect: elements that just became visible or invisible due to the new clip require a full re-render. Same for a changed layer: its opacity/blur re-tints every command inside it (which all compare equal and would contribute nothing), so an animating layer would otherwise never repaint.
            if matches!(
                new_cmd,
                DrawCommand::PushClip { .. } | DrawCommand::PushLayer { .. }
            ) {
                return None;
            }
            if let Some(r) = visual_rect(new_cmd, new_matrix) {
                push_dirty_rect(&mut dirty, r);
            }
            if let Some(r) = visual_rect(old_cmd, old_matrix) {
                push_dirty_rect(&mut dirty, r);
            }
        } else {
            // Content is identical but the on-screen position may have changed because a parent PushMatrix changed. Capture both rects so that old pixels are cleared and the element is re-drawn at the new position.
            let new_r = visual_rect(new_cmd, new_matrix);
            let old_r = visual_rect(old_cmd, old_matrix);
            if new_r != old_r {
                if let Some(r) = new_r {
                    push_dirty_rect(&mut dirty, r);
                }
                if let Some(r) = old_r {
                    push_dirty_rect(&mut dirty, r);
                }
            }
        }
    }

    // Nothing changed visually: report None (same as before) rather than an empty list, so the caller's "no dirty region" path is preserved.
    if dirty.is_empty() { None } else { Some(dirty) }
}

/// Detect whether the only change between two command slices is a pure axis-aligned (X-only or Y-only) translation of scrollable content within a fixed clip.
pub fn detect_scroll_blit(
    new_cmds: &[DrawCommand],
    old_cmds: &[DrawCommand],
) -> Option<ScrollBlit> {
    if new_cmds.len() != old_cmds.len() {
        return None;
    }

    let n = new_cmds.len();

    // Find the first position where commands differ; must be a PushMatrix encoding a pure axis-aligned translation.
    let scroll_idx = new_cmds
        .iter()
        .zip(old_cmds.iter())
        .position(|(nc, oc)| nc != oc)?;

    let (delta_x_f, delta_y_f) = match (&new_cmds[scroll_idx], &old_cmds[scroll_idx]) {
        (DrawCommand::PushMatrix { matrix: nm }, DrawCommand::PushMatrix { matrix: om }) => {
            match (matrix_as_translation(nm), matrix_as_translation(om)) {
                (Some((ntx, nty)), Some((otx, oty))) if ntx == otx => (0.0f32, nty - oty),
                (Some((ntx, nty)), Some((otx, oty))) if nty == oty => (ntx - otx, 0.0f32),
                _ => return None,
            }
        }
        _ => return None,
    };

    // Reconstruct clip stack at scroll_idx to determine the scroll viewport.
    let mut clip_stack: Vec<Rect> = Vec::new();
    for cmd in &new_cmds[..scroll_idx] {
        match cmd {
            DrawCommand::PushClip { rect, .. } => {
                let effective = clip_stack
                    .last()
                    .and_then(|&c| c.intersect(*rect))
                    .unwrap_or(*rect);
                clip_stack.push(effective);
            }
            DrawCommand::PopClip => {
                clip_stack.pop();
            }
            _ => {}
        }
    }

    let scroll_clip = *clip_stack.last()?;

    let delta_x = delta_x_f as i32;
    let delta_y = delta_y_f as i32;

    // No savings from blitting if the entire clip would need repaint.
    if delta_x != 0 && (delta_x.abs() as f32) >= scroll_clip.width {
        return None;
    }
    if delta_y != 0 && (delta_y.abs() as f32) >= scroll_clip.height {
        return None;
    }

    let (dx_f, dy_f) = (delta_x as f32, delta_y as f32);
    // Regions the blit displaced that must be repainted in place (see ScrollBlit::extra_dirty).
    let mut extra_dirty: SmallVec<[Rect; 8]> = SmallVec::new();

    // Static visuals drawn BEFORE the scroll PushTransform (fixed headers, separators) sit inside scroll_clip, so the blit shifts their pixels. They are unchanged (scroll_idx is the first diff), so repaint each in place (plus its ghost) instead of bailing.
    for c in &new_cmds[..scroll_idx] {
        let r = culling::command_visual_rect(c, IDENTITY_MATRIX, &FontMetrics::default());
        if let Some(region) = displaced_region(r, r, dx_f, dy_f) {
            extra_dirty.push(region);
            if extra_dirty.len() > MAX_SCROLL_EXTRA_DIRTY {
                return None;
            }
        }
    }

    // Find the PopMatrix that closes the scroll PushTransform; PushMatrix nesting also counts.
    let mut depth = 1i32;
    let mut pop_idx = None;
    let mut i = scroll_idx + 1;
    while i < n {
        match &new_cmds[i] {
            DrawCommand::PushMatrix { .. } => depth += 1,
            DrawCommand::PopMatrix => {
                depth -= 1;
                if depth == 0 {
                    pop_idx = Some(i);
                    break;
                }
            }
            _ => {}
        }
        i += 1;
    }

    let pop_idx = pop_idx?;

    // All commands inside the scroll region must be structurally identical; only the top-level translate may differ, so the blit is a valid optimisation.
    for j in (scroll_idx + 1)..pop_idx {
        if new_cmds[j] != old_cmds[j] {
            return None;
        }
    }

    let exposed_band = if delta_x != 0 {
        if delta_x < 0 {
            // Content moved left (scrolled right): the right strip is newly exposed.
            let band_w = (-delta_x) as f32;
            Rect::new(
                scroll_clip.x + scroll_clip.width - band_w,
                scroll_clip.y,
                band_w,
                scroll_clip.height,
            )
        } else {
            // Content moved right (scrolled left): the left strip is newly exposed.
            let band_w = delta_x as f32;
            Rect::new(scroll_clip.x, scroll_clip.y, band_w, scroll_clip.height)
        }
    } else if delta_y < 0 {
        // Content moved up (scrolled down): the bottom band is newly exposed.
        let band_h = (-delta_y) as f32;
        Rect::new(
            scroll_clip.x,
            scroll_clip.y + scroll_clip.height - band_h,
            scroll_clip.width,
            band_h,
        )
    } else {
        // Content moved down (scrolled up): the top band is newly exposed.
        let band_h = delta_y as f32;
        Rect::new(scroll_clip.x, scroll_clip.y, scroll_clip.width, band_h)
    };

    // Walk a single DrawState through the scroll block (0..=pop_idx) to inherit the outer matrix context, then keep advancing it inline over the suffix — no Vec of matrices for the whole slice.
    let mut state = DrawState::new();
    for cmd in &new_cmds[..=pop_idx] {
        advance_matrix(&mut state, cmd);
    }

    // Repaint overlays and static elements after the scroll block (scrollbar, fixed footers, dev overlays): redraw each at its current position and repaint the scrolled content under the ghost the blit shifted its previous pixels to. Unchanged elements here used to force a full re-render.
    for j in (pop_idx + 1)..n {
        advance_matrix(&mut state, &new_cmds[j]);
        let cmd_matrix = state.cumulative_matrix;
        let new_r = culling::command_visual_rect(&new_cmds[j], cmd_matrix, &FontMetrics::default());
        let old_r = culling::command_visual_rect(&old_cmds[j], cmd_matrix, &FontMetrics::default());
        if let Some(region) = displaced_region(new_r, old_r, dx_f, dy_f) {
            extra_dirty.push(region);
            if extra_dirty.len() > MAX_SCROLL_EXTRA_DIRTY {
                return None;
            }
        }
    }

    Some(ScrollBlit {
        scroll_clip,
        delta_x,
        delta_y,
        exposed_band,
        extra_dirty,
    })
}

#[cfg(test)]
mod tests {
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
            .is_none()
        );
    }

    // Regression: an opacity-only PushLayer change must force a full re-render (None). The layer has no geometry and its inner commands compare equal, so treating it like a normal changed command yields an empty dirty list and the animating layer never repaints.
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
            .is_none()
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
        // overlapping old/new positions merge into a single region covering both
        let dirty = rects.iter().copied().reduce(Rect::union).unwrap();
        assert!(dirty.x <= 0.0);
        assert!(dirty.x + dirty.width >= 15.0);
    }

    #[test]
    fn compute_dirty_rect_disjoint_changes_stay_separate() {
        // A change at the top-left and a far-away change at the bottom-right must remain two disjoint regions, not collapse into a viewport-spanning union.
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
        // Neither region should span the gap between the two corners.
        for r in &rects {
            assert!(r.width < 100.0 && r.height < 100.0);
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
        // must cover both positions
        assert!(dirty.x <= 0.0);
        assert!(dirty.y <= 0.0);
        assert!(dirty.x + dirty.width >= 15.0);
        assert!(dirty.y + dirty.height >= 15.0);
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
            .is_none()
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
        assert!(detect_scroll_blit(&cmds, &cmds).is_none());
    }

    #[test]
    fn detect_scroll_blit_repaints_static_visual_before_scroll() {
        // A static element before the scroll PushTransform (e.g. a header) lives inside scroll_clip, so the blit shifts its pixels. detect_scroll_blit keeps the blit and repaints the header (plus its ghost) via extra_dirty instead of bailing to a full re-render.
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
        // The header at (0,0,100,30) must fall inside an extra-dirty region so it is repainted in place.
        let covers_header = sb
            .extra_dirty
            .iter()
            .any(|r| r.x <= 50.0 && r.x + r.width >= 50.0 && r.y <= 15.0 && r.y + r.height >= 15.0);
        assert!(covers_header, "header not repainted: {:?}", sb.extra_dirty);
    }

    #[test]
    fn detect_scroll_blit_repaints_static_visual_after_scroll() {
        // A static element after the scroll PopMatrix (e.g. a footer or dev overlay) is inside scroll_clip and gets shifted by the blit. detect_scroll_blit repaints it (plus its ghost) via extra_dirty instead of bailing.
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
        // The footer at (0,170,100,30) must fall inside an extra-dirty region so it is repainted in place.
        let covers_footer = sb.extra_dirty.iter().any(|r| {
            r.x <= 50.0 && r.x + r.width >= 50.0 && r.y <= 185.0 && r.y + r.height >= 185.0
        });
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
        // Inner rect composes to (10, 10) in old and (10, 60) in new.
        assert!(dirty.y <= 10.0);
        assert!(dirty.y + dirty.height >= 70.0);
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
        // bottom band exposed when scrolling down
        assert_eq!(blit.exposed_band.y, 190.0);
        assert_eq!(blit.exposed_band.height, 10.0);
    }
}
