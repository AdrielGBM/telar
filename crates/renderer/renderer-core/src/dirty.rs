use geometry_core::Rect;

use crate::{DrawCommand, culling};

/// When a pure Y-scroll is detected, this describes what changed.
pub struct ScrollBlit {
    /// The clipping rect that encloses the scrollable content.
    pub scroll_clip: Rect,
    /// How many pixels the content shifted (negative = content moved up = scroll down).
    pub delta_ty: i32,
    /// The band of newly exposed pixels that must be re-rendered.
    pub exposed_band: Rect,
    /// Bounds of any other changed elements outside the scroll clip (e.g. scrollbar).
    pub extra_dirty: Option<Rect>,
}

fn union_rects(a: Rect, b: Rect) -> Rect {
    let x = a.x.min(b.x);
    let y = a.y.min(b.y);
    let right = (a.x + a.width).max(b.x + b.width);
    let bottom = (a.y + a.height).max(b.y + b.height);
    Rect::new(x, y, right - x, bottom - y)
}

/// Compare two consecutive DrawCommand slices and return the union of regions
/// that changed visually. Returns None if a full re-render is required.
///
/// Requires the caller to provide a function that computes the on-screen
/// bounding rect of a DrawCommand given cumulative translation offsets.
pub fn compute_dirty_rect(
    new_cmds: &[DrawCommand],
    old_cmds: &[DrawCommand],
    visual_rect: impl Fn(&DrawCommand, f32, f32) -> Option<Rect>,
) -> Option<Rect> {
    if new_cmds.len() != old_cmds.len() {
        return None;
    }

    let mut dirty: Option<Rect> = None;
    let mut new_tx_stack: Vec<(f32, f32)> = Vec::new();
    let mut old_tx_stack: Vec<(f32, f32)> = Vec::new();
    let mut new_cum_tx = 0.0f32;
    let mut new_cum_ty = 0.0f32;
    let mut old_cum_tx = 0.0f32;
    let mut old_cum_ty = 0.0f32;

    for (new_cmd, old_cmd) in new_cmds.iter().zip(old_cmds.iter()) {
        match new_cmd {
            DrawCommand::PushTransform { tx, ty } => {
                new_tx_stack.push((*tx, *ty));
                new_cum_tx += tx;
                new_cum_ty += ty;
            }
            DrawCommand::PopTransform => {
                if let Some((tx, ty)) = new_tx_stack.pop() {
                    new_cum_tx -= tx;
                    new_cum_ty -= ty;
                }
            }
            _ => {}
        }
        match old_cmd {
            DrawCommand::PushTransform { tx, ty } => {
                old_tx_stack.push((*tx, *ty));
                old_cum_tx += tx;
                old_cum_ty += ty;
            }
            DrawCommand::PopTransform => {
                if let Some((tx, ty)) = old_tx_stack.pop() {
                    old_cum_tx -= tx;
                    old_cum_ty -= ty;
                }
            }
            _ => {}
        }

        if new_cmd != old_cmd {
            if let Some(r) = visual_rect(new_cmd, new_cum_tx, new_cum_ty) {
                dirty = Some(dirty.map_or(r, |d| union_rects(d, r)));
            }
            if let Some(r) = visual_rect(old_cmd, old_cum_tx, old_cum_ty) {
                dirty = Some(dirty.map_or(r, |d| union_rects(d, r)));
            }
        } else {
            // Content is identical but the on-screen position may have changed because a parent
            // PushTransform changed. Capture both rects so that old pixels are cleared and the
            // element is re-drawn at the new position.
            let new_r = visual_rect(new_cmd, new_cum_tx, new_cum_ty);
            let old_r = visual_rect(old_cmd, old_cum_tx, old_cum_ty);
            if new_r != old_r {
                if let Some(r) = new_r {
                    dirty = Some(dirty.map_or(r, |d| union_rects(d, r)));
                }
                if let Some(r) = old_r {
                    dirty = Some(dirty.map_or(r, |d| union_rects(d, r)));
                }
            }
        }
    }

    dirty
}

/// Detect whether the only change between two command slices is a pure Y-axis
/// translation of scrollable content within a fixed clip. Returns a ScrollBlit
/// descriptor if scroll blit can be applied, or None otherwise.
pub fn detect_scroll_blit(
    new_cmds: &[DrawCommand],
    old_cmds: &[DrawCommand],
) -> Option<ScrollBlit> {
    if new_cmds.len() != old_cmds.len() {
        return None;
    }

    let n = new_cmds.len();

    // Find the first position where commands differ; must be a PushTransform with only ty changed.
    let scroll_idx = new_cmds
        .iter()
        .zip(old_cmds.iter())
        .position(|(nc, oc)| nc != oc)?;

    let (new_ty, old_ty) = match (&new_cmds[scroll_idx], &old_cmds[scroll_idx]) {
        (
            DrawCommand::PushTransform { tx: ntx, ty: nty },
            DrawCommand::PushTransform { tx: otx, ty: oty },
        ) if ntx == otx => (*nty, *oty),
        _ => return None,
    };

    // Reconstruct clip stack at scroll_idx to determine the scroll viewport.
    let mut clip_stack: Vec<Rect> = Vec::new();
    for cmd in &new_cmds[..scroll_idx] {
        match cmd {
            DrawCommand::PushClip { rect } => {
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

    let delta_ty_f = new_ty - old_ty;
    let delta_ty = delta_ty_f as i32;

    // No savings from blitting if the entire clip would need repaint.
    if (delta_ty.abs() as f32) >= scroll_clip.height {
        return None;
    }

    // Find the PopTransform that closes the scroll PushTransform.
    let mut depth = 1i32;
    let mut pop_idx = None;
    let mut i = scroll_idx + 1;
    while i < n {
        match &new_cmds[i] {
            DrawCommand::PushTransform { .. } => depth += 1,
            DrawCommand::PopTransform => {
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

    // All commands inside the scroll region must be structurally identical; only the
    // top-level translate may differ, so the blit is a valid optimisation.
    for j in (scroll_idx + 1)..pop_idx {
        if new_cmds[j] != old_cmds[j] {
            return None;
        }
    }

    let exposed_band = if delta_ty < 0 {
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

    // Reconstruct cumulative transform at the end of the scroll block.
    let mut tx_stack: Vec<(f32, f32)> = Vec::new();
    let mut cum_tx = 0.0f32;
    let mut cum_ty = 0.0f32;
    for cmd in new_cmds[..=pop_idx].iter() {
        match cmd {
            DrawCommand::PushTransform { tx, ty } => {
                tx_stack.push((*tx, *ty));
                cum_tx += tx;
                cum_ty += ty;
            }
            DrawCommand::PopTransform => {
                if let Some((tx, ty)) = tx_stack.pop() {
                    cum_tx -= tx;
                    cum_ty -= ty;
                }
            }
            _ => {}
        }
    }

    // Collect dirty rects for any overlay changes after the scroll block (e.g. scrollbar).
    let mut extra_dirty: Option<Rect> = None;
    for j in (pop_idx + 1)..n {
        match &new_cmds[j] {
            DrawCommand::PushTransform { tx, ty } => {
                tx_stack.push((*tx, *ty));
                cum_tx += tx;
                cum_ty += ty;
            }
            DrawCommand::PopTransform => {
                if let Some((tx, ty)) = tx_stack.pop() {
                    cum_tx -= tx;
                    cum_ty -= ty;
                }
            }
            _ => {}
        }
        if new_cmds[j] != old_cmds[j] {
            if let Some(r) = culling::command_visual_rect(&new_cmds[j], cum_tx, cum_ty) {
                extra_dirty = Some(extra_dirty.map_or(r, |d| union_rects(d, r)));
            }
        }
    }

    Some(ScrollBlit {
        scroll_clip,
        delta_ty,
        exposed_band,
        extra_dirty,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DrawCommand, style::RectStyle};
    use geometry_core::Rect;

    fn rect_cmd(x: f32, y: f32, w: f32, h: f32) -> DrawCommand {
        DrawCommand::Rect(Box::new(crate::RectPayload {
            rect: Rect::new(x, y, w, h),
            style: RectStyle::default(),
        }))
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
    fn compute_dirty_rect_changed_rect() {
        let old = vec![rect_cmd(0.0, 0.0, 10.0, 10.0)];
        let new = vec![rect_cmd(5.0, 5.0, 10.0, 10.0)];
        let dirty = compute_dirty_rect(&new, &old, culling::command_visual_rect).unwrap();
        // must cover both positions
        assert!(dirty.x <= 0.0);
        assert!(dirty.y <= 0.0);
        assert!(dirty.x + dirty.width >= 15.0);
        assert!(dirty.y + dirty.height >= 15.0);
    }

    #[test]
    fn detect_scroll_blit_no_change_returns_none() {
        let cmds = vec![
            DrawCommand::PushClip {
                rect: Rect::new(0.0, 0.0, 100.0, 200.0),
            },
            DrawCommand::PushTransform { tx: 0.0, ty: -50.0 },
            rect_cmd(0.0, 0.0, 100.0, 400.0),
            DrawCommand::PopTransform,
            DrawCommand::PopClip,
        ];
        assert!(detect_scroll_blit(&cmds, &cmds).is_none());
    }

    #[test]
    fn detect_scroll_blit_pure_y_scroll() {
        let old = vec![
            DrawCommand::PushClip {
                rect: Rect::new(0.0, 0.0, 100.0, 200.0),
            },
            DrawCommand::PushTransform { tx: 0.0, ty: -50.0 },
            rect_cmd(0.0, 0.0, 100.0, 400.0),
            DrawCommand::PopTransform,
            DrawCommand::PopClip,
        ];
        let new = vec![
            DrawCommand::PushClip {
                rect: Rect::new(0.0, 0.0, 100.0, 200.0),
            },
            DrawCommand::PushTransform { tx: 0.0, ty: -60.0 },
            rect_cmd(0.0, 0.0, 100.0, 400.0),
            DrawCommand::PopTransform,
            DrawCommand::PopClip,
        ];
        let blit = detect_scroll_blit(&new, &old).unwrap();
        assert_eq!(blit.delta_ty, -10);
        // bottom band exposed when scrolling down
        assert_eq!(blit.exposed_band.y, 190.0);
        assert_eq!(blit.exposed_band.height, 10.0);
    }
}
