use geometry_core::Rect;
use smallvec::{SmallVec, smallvec};

use crate::{
    DrawCommand, culling,
    draw_state::{IDENTITY_MATRIX, compose_matrix},
    geometry::union_rects,
};

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
        let mut merged = union_rects(rects[idx], r);
        rects.swap_remove(idx);
        // The merged rect may now touch other entries; keep folding until it is disjoint from all of them.
        let mut i = 0;
        while i < rects.len() {
            if rects_adjacent_or_overlapping(rects[i], merged, SLOP) {
                merged = union_rects(rects[i], merged);
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
            .reduce(union_rects)
            .expect("non-empty");
        *rects = smallvec![union];
    }
}

/// When a pure axis-aligned scroll is detected, this describes what changed.
pub struct ScrollBlit {
    /// The clipping rect that encloses the scrollable content.
    pub scroll_clip: Rect,
    /// Horizontal pixel shift (negative = content moved left = scroll right). Zero for Y-only scrolls.
    pub delta_tx: i32,
    /// Vertical pixel shift (negative = content moved up = scroll down). Zero for X-only scrolls.
    pub delta_ty: i32,
    /// The strip of newly exposed pixels that must be re-rendered (horizontal band for Y scrolls, vertical band for X scrolls).
    pub exposed_band: Rect,
    /// Bounds of any other changed elements outside the scroll clip (e.g. scrollbar).
    pub extra_dirty: Option<Rect>,
}

fn matrix_as_translation(m: &[f32; 6]) -> Option<(f32, f32)> {
    if m[0] == 1.0 && m[1] == 0.0 && m[2] == 0.0 && m[3] == 1.0 {
        Some((m[4], m[5]))
    } else {
        None
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
    let mut new_matrix_stack: Vec<[f32; 6]> = Vec::new();
    let mut old_matrix_stack: Vec<[f32; 6]> = Vec::new();
    let mut new_matrix = IDENTITY_MATRIX;
    let mut old_matrix = IDENTITY_MATRIX;

    for (new_cmd, old_cmd) in new_cmds.iter().zip(old_cmds.iter()) {
        match new_cmd {
            DrawCommand::PushMatrix { matrix } => {
                new_matrix_stack.push(new_matrix);
                new_matrix = compose_matrix(new_matrix, *matrix);
            }
            DrawCommand::PopMatrix => {
                if let Some(prev) = new_matrix_stack.pop() {
                    new_matrix = prev;
                }
            }
            _ => {}
        }
        match old_cmd {
            DrawCommand::PushMatrix { matrix } => {
                old_matrix_stack.push(old_matrix);
                old_matrix = compose_matrix(old_matrix, *matrix);
            }
            DrawCommand::PopMatrix => {
                if let Some(prev) = old_matrix_stack.pop() {
                    old_matrix = prev;
                }
            }
            _ => {}
        }

        if new_cmd != old_cmd {
            // A changed clip boundary cannot be expressed as a bounded dirty rect: elements that just became visible or invisible due to the new clip require a full re-render.
            if matches!(new_cmd, DrawCommand::PushClip { .. }) {
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

    let (delta_tx_f, delta_ty_f) = match (&new_cmds[scroll_idx], &old_cmds[scroll_idx]) {
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

    // apply_scroll_blit shifts every pixel row inside scroll_clip. Visual commands that sit before the scroll PushTransform (headers, separators, etc.) are always identical to their prev-frame counterparts (scroll_idx is the first diff), so they will never land in extra_dirty and will never be redrawn — their pixels drift with each scroll step until they disappear. Bail out to compute_dirty_rect when any such element exists.
    if new_cmds[..scroll_idx]
        .iter()
        .any(|c| culling::command_visual_rect(c, IDENTITY_MATRIX).is_some())
    {
        return None;
    }

    let delta_tx = delta_tx_f as i32;
    let delta_ty = delta_ty_f as i32;

    // No savings from blitting if the entire clip would need repaint.
    if delta_tx != 0 && (delta_tx.abs() as f32) >= scroll_clip.width {
        return None;
    }
    if delta_ty != 0 && (delta_ty.abs() as f32) >= scroll_clip.height {
        return None;
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

    let exposed_band = if delta_tx != 0 {
        if delta_tx < 0 {
            // Content moved left (scrolled right): the right strip is newly exposed.
            let band_w = (-delta_tx) as f32;
            Rect::new(
                scroll_clip.x + scroll_clip.width - band_w,
                scroll_clip.y,
                band_w,
                scroll_clip.height,
            )
        } else {
            // Content moved right (scrolled left): the left strip is newly exposed.
            let band_w = delta_tx as f32;
            Rect::new(scroll_clip.x, scroll_clip.y, band_w, scroll_clip.height)
        }
    } else if delta_ty < 0 {
        // Content moved up (scrolled down): the bottom band is newly exposed.
        let band_h = (-delta_ty) as f32;
        Rect::new(
            scroll_clip.x,
            scroll_clip.y + scroll_clip.height - band_h,
            scroll_clip.width,
            band_h,
        )
    } else {
        // Content moved down (scrolled up): the top band is newly exposed.
        let band_h = delta_ty as f32;
        Rect::new(scroll_clip.x, scroll_clip.y, scroll_clip.width, band_h)
    };

    // Reconstruct cumulative matrix at the end of the scroll block.
    let mut matrix_stack: Vec<[f32; 6]> = Vec::new();
    let mut cum_matrix = IDENTITY_MATRIX;
    for cmd in new_cmds[..=pop_idx].iter() {
        match cmd {
            DrawCommand::PushMatrix { matrix } => {
                matrix_stack.push(cum_matrix);
                cum_matrix = compose_matrix(cum_matrix, *matrix);
            }
            DrawCommand::PopMatrix => {
                if let Some(prev) = matrix_stack.pop() {
                    cum_matrix = prev;
                }
            }
            _ => {}
        }
    }

    // Collect dirty rects for any overlay changes after the scroll block (e.g. scrollbar).
    let mut extra_dirty: Option<Rect> = None;
    for j in (pop_idx + 1)..n {
        match &new_cmds[j] {
            DrawCommand::PushMatrix { matrix } => {
                matrix_stack.push(cum_matrix);
                cum_matrix = compose_matrix(cum_matrix, *matrix);
            }
            DrawCommand::PopMatrix => {
                if let Some(prev) = matrix_stack.pop() {
                    cum_matrix = prev;
                }
            }
            _ => {}
        }
        if new_cmds[j] != old_cmds[j] {
            if let Some(r) = culling::command_visual_rect(&new_cmds[j], cum_matrix) {
                extra_dirty = Some(extra_dirty.map_or(r, |d| union_rects(d, r)));
            }
        } else if culling::command_visual_rect(&new_cmds[j], cum_matrix).is_some() {
            // Unchanged visual after the scroll block: blit shifted its pixels but it won't land in extra_dirty, so it will never be redrawn at the correct position.
            return None;
        }
    }

    Some(ScrollBlit {
        scroll_clip,
        delta_tx,
        delta_ty,
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
        assert!(compute_dirty_rect(&a, &b, culling::command_visual_rect).is_none());
    }

    #[test]
    fn compute_dirty_rect_no_change_returns_none() {
        let a = vec![rect_cmd(0.0, 0.0, 10.0, 10.0)];
        assert!(compute_dirty_rect(&a, &a, culling::command_visual_rect).is_none());
    }

    #[test]
    fn compute_dirty_rect_single_change() {
        let old = vec![rect_cmd(0.0, 0.0, 10.0, 10.0)];
        let new = vec![rect_cmd(5.0, 0.0, 10.0, 10.0)];
        let rects = compute_dirty_rect(&new, &old, culling::command_visual_rect).unwrap();
        // overlapping old/new positions merge into a single region covering both
        let dirty = rects.iter().copied().reduce(union_rects).unwrap();
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
        let rects = compute_dirty_rect(&new, &old, culling::command_visual_rect).unwrap();
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
        let rects = compute_dirty_rect(&new, &old, culling::command_visual_rect).unwrap();
        let dirty = rects.iter().copied().reduce(union_rects).unwrap();
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
        assert!(compute_dirty_rect(&new, &old, culling::command_visual_rect).is_none());
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
    fn detect_scroll_blit_rejects_visual_before_scroll() {
        // A visual element before the scroll PushTransform (e.g. a header) lives inside scroll_clip; apply_scroll_blit would shift its pixels without ever redrawing it.
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
        assert!(detect_scroll_blit(&new, &old).is_none());
    }

    #[test]
    fn detect_scroll_blit_rejects_unchanged_visual_after_scroll() {
        // An unchanged visual element after the scroll PopMatrix (e.g. a footer) would have its pixels shifted by apply_scroll_blit and never redrawn.
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
        assert!(detect_scroll_blit(&new, &old).is_none());
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
        assert_eq!(blit.delta_ty, -10);
        // bottom band exposed when scrolling down
        assert_eq!(blit.exposed_band.y, 190.0);
        assert_eq!(blit.exposed_band.height, 10.0);
    }
}
