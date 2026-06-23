use geometry_core::Rect;

use crate::DrawCommand;

/// Font ascender/line-height metrics expressed as ratios relative to `font_size`.
/// Default values are conservative approximations that hold for most common fonts.
#[derive(Clone, Copy)]
pub struct FontMetrics {
    /// Multiplier for line height: `font_size * line_height_factor` gives the full line height.
    pub line_height_factor: f32,
    /// Fraction of `font_size` by which glyphs can extend above the rect's top edge (ascender overshoot).
    pub ascender_ratio: f32,
}

impl Default for FontMetrics {
    fn default() -> Self {
        Self {
            line_height_factor: 1.2,
            ascender_ratio: 0.25,
        }
    }
}

pub fn overlaps(x: f32, y: f32, w: f32, h: f32, clip: Option<Rect>) -> bool {
    match clip {
        None => true,
        Some(c) => Rect::new(x, y, w, h).overlaps(c),
    }
}

pub fn expand_for_shadow(
    rect: Rect,
    blur_radius: f32,
    spread: f32,
    offset_x: f32,
    offset_y: f32,
) -> Rect {
    // Must match the software renderer's blur padding (ceil(blur_radius * 1.5) + 1); using only blur_radius leaves stale shadow pixels outside the dirty rect when shadows move.
    let expand = (blur_radius * 1.5).ceil() + 1.0 + spread;
    let expanded = Rect::new(
        rect.x - expand,
        rect.y - expand,
        rect.width + expand * 2.0,
        rect.height + expand * 2.0,
    );
    let shifted = Rect::new(
        expanded.x + offset_x,
        expanded.y + offset_y,
        expanded.width,
        expanded.height,
    );
    rect.union(shifted)
}

#[inline]
pub fn apply_matrix(matrix: [f32; 6], x: f32, y: f32) -> (f32, f32) {
    let [a, b, c, d, e, f] = matrix;
    (a * x + c * y + e, b * x + d * y + f)
}

#[inline]
fn transform_rect_aabb(matrix: [f32; 6], rx: f32, ry: f32, rw: f32, rh: f32) -> Rect {
    let (x0, y0) = apply_matrix(matrix, rx, ry);
    let (x1, y1) = apply_matrix(matrix, rx + rw, ry);
    let (x2, y2) = apply_matrix(matrix, rx, ry + rh);
    let (x3, y3) = apply_matrix(matrix, rx + rw, ry + rh);
    let min_x = x0.min(x1).min(x2).min(x3);
    let min_y = y0.min(y1).min(y2).min(y3);
    let max_x = x0.max(x1).max(x2).max(x3);
    let max_y = y0.max(y1).max(y2).max(y3);
    Rect::new(min_x, min_y, max_x - min_x, max_y - min_y)
}

pub fn command_visual_rect(
    cmd: &DrawCommand,
    matrix: [f32; 6],
    font_metrics: &FontMetrics,
) -> Option<Rect> {
    match cmd {
        DrawCommand::Rect { rect, style } => {
            let r = transform_rect_aabb(matrix, rect.x, rect.y, rect.width, rect.height);
            let shadow = style.shadow;
            Some(match shadow {
                Some(s) => expand_for_shadow(r, s.blur_radius, s.spread, s.offset_x, s.offset_y),
                None => r,
            })
        }
        DrawCommand::Text { rect, style, .. } => {
            // Glyphs can extend outside rect: ascenders above rect.y and the line height may exceed rect.height. Expand the visual rect to cover the real glyph extent so that dirty-rect computation and culling never under-estimate the painted area.
            let font_size = style.font_size;
            let shadow = style.shadow;
            let line_h = font_size * font_metrics.line_height_factor;
            let ascender_overshoot = font_size * font_metrics.ascender_ratio;
            let extra_bottom = (line_h - rect.height).max(0.0);
            let r = transform_rect_aabb(
                matrix,
                rect.x,
                rect.y - ascender_overshoot,
                rect.width,
                rect.height + ascender_overshoot + extra_bottom,
            );
            Some(match shadow {
                Some(s) => expand_for_shadow(r, s.blur_radius, s.spread, s.offset_x, s.offset_y),
                None => r,
            })
        }
        DrawCommand::Image { rect, .. } => Some(transform_rect_aabb(
            matrix,
            rect.x,
            rect.y,
            rect.width,
            rect.height,
        )),
        DrawCommand::Line { p1, p2, style } => {
            let half_w = style.width / 2.0;
            let (tx1, ty1) = apply_matrix(matrix, p1.x, p1.y);
            let (tx2, ty2) = apply_matrix(matrix, p2.x, p2.y);
            let x = tx1.min(tx2) - half_w;
            let y = ty1.min(ty2) - half_w;
            let right = tx1.max(tx2) + half_w;
            let bottom = ty1.max(ty2) + half_w;
            Some(Rect::new(x, y, right - x, bottom - y))
        }
        DrawCommand::Path { data, style } => {
            let base = data.bounds()?;
            let r = transform_rect_aabb(matrix, base.x, base.y, base.width, base.height);
            let shadow = style.shadow;
            Some(match shadow {
                Some(s) => expand_for_shadow(r, s.blur_radius, s.spread, s.offset_x, s.offset_y),
                None => r,
            })
        }
        DrawCommand::PushClip { .. }
        | DrawCommand::PopClip
        | DrawCommand::PushMatrix { .. }
        | DrawCommand::PopMatrix
        | DrawCommand::PushLayer { .. }
        | DrawCommand::PopLayer => None,
        #[cfg(target_os = "android")]
        DrawCommand::AndroidHardwareBufferImage { .. } => None,
    }
}

pub fn extend_bounds(current: Option<Rect>, new_rect: Rect) -> Option<Rect> {
    Some(current.map_or(new_rect, |b| b.union(new_rect)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw_state::IDENTITY_MATRIX;

    #[test]
    fn overlaps_no_clip() {
        assert!(overlaps(0.0, 0.0, 10.0, 10.0, None));
    }

    #[test]
    fn overlaps_inside_clip() {
        let clip = Rect::new(0.0, 0.0, 100.0, 100.0);
        assert!(overlaps(10.0, 10.0, 20.0, 20.0, Some(clip)));
    }

    #[test]
    fn overlaps_outside_clip() {
        let clip = Rect::new(0.0, 0.0, 10.0, 10.0);
        assert!(!overlaps(20.0, 20.0, 5.0, 5.0, Some(clip)));
    }

    #[test]
    fn expand_for_shadow_expands_all_sides() {
        let r = Rect::new(10.0, 10.0, 20.0, 20.0);
        let result = expand_for_shadow(r, 5.0, 2.0, 0.0, 0.0);
        assert!(result.x < r.x);
        assert!(result.y < r.y);
        assert!(result.x + result.width > r.x + r.width);
        assert!(result.y + result.height > r.y + r.height);
    }

    #[test]
    fn transform_rect_aabb_identity() {
        let r = transform_rect_aabb(IDENTITY_MATRIX, 10.0, 20.0, 30.0, 40.0);
        assert_eq!(r.x, 10.0);
        assert_eq!(r.y, 20.0);
        assert_eq!(r.width, 30.0);
        assert_eq!(r.height, 40.0);
    }

    #[test]
    fn transform_rect_aabb_scale() {
        let scale = [2.0, 0.0, 0.0, 2.0, 0.0, 0.0];
        let r = transform_rect_aabb(scale, 5.0, 5.0, 10.0, 10.0);
        assert!((r.x - 10.0).abs() < 1e-4);
        assert!((r.y - 10.0).abs() < 1e-4);
        assert!((r.width - 20.0).abs() < 1e-4);
        assert!((r.height - 20.0).abs() < 1e-4);
    }
}
