//! What a command paints, in window space — the one answer both dirty tracking and layer sizing ask for.

use geometry_core::{Rect, Transform};

use crate::DrawCommand;
use crate::transform_clip_rect;

/// Font ascender/line-height metrics expressed as ratios relative to `font_size`. Default values are conservative approximations that hold for most common fonts.
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

/// Whether a rect intersects `clip`. A `None` clip clips nothing, so everything overlaps it.
pub fn overlaps(x: f32, y: f32, w: f32, h: f32, clip: Option<Rect>) -> bool {
    match clip {
        None => true,
        Some(c) => Rect::new(x, y, w, h).overlaps(c),
    }
}

/// Grows a rect by the room a shadow needs around it, offset included.
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

/// What a command paints, in window space, or `None` for one that paints nothing.
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
            let shadow = style.text_shadow.cast();
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
            // A stroke straddles the path, reaching half its width past the geometry on every side. Left out, the damage rect is short by that half and a moving stroke's outer edge is never repainted, leaving a trail.
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
        | DrawCommand::PopLayer
        | DrawCommand::PushElement { .. }
        | DrawCommand::PopElement => None,
    }
}

/// Unions `new_rect` into `current`, starting the accumulation when it is `None`.
pub fn extend_bounds(current: Option<Rect>, new_rect: Rect) -> Option<Rect> {
    Some(current.map_or(new_rect, |b| b.union(new_rect)))
}

#[cfg(test)]
#[path = "culling_test.rs"]
mod tests;
