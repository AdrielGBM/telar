use geometry_core::{Rect, Transform};

use crate::DrawCommand;
use crate::transform_clip_rect;

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
    // Through the renderers' own padding: a smaller expansion here leaves stale shadow pixels outside the dirty rect when a shadow moves.
    let expand =
        crate::preprocess::blur_padding(crate::preprocess::blur_sigma(blur_radius)) as f32 + spread;
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

pub fn command_visual_rect(
    cmd: &DrawCommand,
    matrix: [f32; 6],
    font_metrics: &FontMetrics,
) -> Option<Rect> {
    match cmd {
        DrawCommand::Rect { rect, style } => {
            let r = transform_clip_rect(matrix, *rect);
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
            let r = transform_clip_rect(
                matrix,
                Rect::new(
                    rect.x,
                    rect.y - ascender_overshoot,
                    rect.width,
                    rect.height + ascender_overshoot + extra_bottom,
                ),
            );
            Some(match shadow {
                Some(s) => expand_for_shadow(r, s.blur_radius, s.spread, s.offset_x, s.offset_y),
                None => r,
            })
        }
        DrawCommand::RichText { rect, base, .. } => {
            // Same glyph overshoot as `Text`, driven by the paragraph's base metrics.
            let font_size = base.font_size;
            let line_h = font_size * font_metrics.line_height_factor;
            let ascender_overshoot = font_size * font_metrics.ascender_ratio;
            let extra_bottom = (line_h - rect.height).max(0.0);
            let r = transform_clip_rect(
                matrix,
                Rect::new(
                    rect.x,
                    rect.y - ascender_overshoot,
                    rect.width,
                    rect.height + ascender_overshoot + extra_bottom,
                ),
            );
            Some(match base.shadow {
                Some(s) => expand_for_shadow(r, s.blur_radius, s.spread, s.offset_x, s.offset_y),
                None => r,
            })
        }
        DrawCommand::Image { rect, .. } => Some(transform_clip_rect(matrix, *rect)),
        DrawCommand::Line { p1, p2, style } => {
            let half_w = style.width / 2.0;
            let t = Transform::from_array(matrix);
            let m1 = t.apply(*p1);
            let m2 = t.apply(*p2);
            let x = m1.x.min(m2.x) - half_w;
            let y = m1.y.min(m2.y) - half_w;
            let right = m1.x.max(m2.x) + half_w;
            let bottom = m1.y.max(m2.y) + half_w;
            Some(Rect::new(x, y, right - x, bottom - y))
        }
        DrawCommand::Path { data, style } => {
            let base = data.bounds()?;
            let r = transform_clip_rect(matrix, base);
            // A stroke straddles the path, so it reaches half its width past the geometry on every
            // side — the same half `Line` above already accounts for. Left out, the damage rect is
            // short by that half and the outer edge of a moving stroke is never repainted, which
            // leaves a trail behind it.
            let r = match style.stroke {
                Some(stroke) => {
                    let half = stroke.width / 2.0;
                    Rect::new(
                        r.x - half,
                        r.y - half,
                        r.width + half * 2.0,
                        r.height + half * 2.0,
                    )
                }
                None => r,
            };
            Some(match style.shadow {
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
    }
}

pub fn extend_bounds(current: Option<Rect>, new_rect: Rect) -> Option<Rect> {
    Some(current.map_or(new_rect, |b| b.union(new_rect)))
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn transform_clip_rect_identity() {
        let r = transform_clip_rect(
            Transform::IDENTITY.to_array(),
            Rect::new(10.0, 20.0, 30.0, 40.0),
        );
        assert_eq!(r.x, 10.0);
        assert_eq!(r.y, 20.0);
        assert_eq!(r.width, 30.0);
        assert_eq!(r.height, 40.0);
    }

    #[test]
    fn transform_clip_rect_scale() {
        let scale = [2.0, 0.0, 0.0, 2.0, 0.0, 0.0];
        let r = transform_clip_rect(scale, Rect::new(5.0, 5.0, 10.0, 10.0));
        assert!((r.x - 10.0).abs() < 1e-4);
        assert!((r.y - 10.0).abs() < 1e-4);
        assert!((r.width - 20.0).abs() < 1e-4);
        assert!((r.height - 20.0).abs() < 1e-4);
    }

    /// A stroke straddles its path, so the visual rect has to reach half the width past the geometry
    /// — as `Line` already does. Without it the damage rect is short by that half, and the outer edge
    /// of a stroke that moves is never repainted: it leaves a trail behind it.
    #[test]
    fn a_stroked_path_reaches_half_its_width_past_its_geometry() {
        use crate::{Color, PathData, PathStyle, Stroke};
        use geometry_core::Point;
        use std::sync::Arc;

        let data = Arc::new(
            PathData::new()
                .move_to(Point::new(10.0, 10.0))
                .line_to(Point::new(40.0, 10.0)),
        );
        let bare = DrawCommand::Path {
            data: Arc::clone(&data),
            style: Arc::new(PathStyle::default()),
        };
        let stroked = DrawCommand::Path {
            data,
            style: Arc::new(PathStyle {
                stroke: Some(Stroke::new(Color::BLACK, 6.0)),
                ..Default::default()
            }),
        };

        let matrix = Transform::IDENTITY.to_array();
        let metrics = FontMetrics::default();
        let bare = command_visual_rect(&bare, matrix, &metrics).expect("a path has bounds");
        let wide =
            command_visual_rect(&stroked, matrix, &metrics).expect("and so does a stroked one");

        assert_eq!(wide.x, bare.x - 3.0);
        assert_eq!(wide.y, bare.y - 3.0);
        assert_eq!(wide.width, bare.width + 6.0);
        assert_eq!(wide.height, bare.height + 6.0);
    }
}
