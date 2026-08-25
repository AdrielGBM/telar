use std::hash::Hasher;

use rustc_hash::FxHasher;

use crate::{
    BorderRadius, BorderWidths, Declared, FillRule, FontFamily, GlyphRaster, Gradient,
    GradientKind, Paint, PathStyle, RectStyle, Shadow, Stroke, TextStyle,
};

// Styles carry f32 fields and enums without a fixed bit layout, so they are not `bytemuck::Pod`; each field is hashed explicitly (f32 via `to_bits` to stay total over NaN) instead.
pub fn hash_rect_style(s: &RectStyle) -> u64 {
    let mut h = FxHasher::default();
    hash_opt_paint(s.fill.as_ref(), &mut h);
    hash_opt_stroke(s.stroke.as_ref(), &mut h);
    hash_opt_shadow(s.shadow.as_ref(), &mut h);
    hash_border_radius(&s.radius, &mut h);
    hash_border_widths(&s.border_widths, &mut h);
    h.finish()
}

fn hash_border_widths(w: &BorderWidths, h: &mut FxHasher) {
    match w {
        BorderWidths::Uniform => h.write_u8(0),
        BorderWidths::PerSide {
            top,
            right,
            bottom,
            left,
        } => {
            h.write_u8(1);
            for v in [top, right, bottom, left] {
                h.write_u32(v.to_bits());
            }
        }
    }
}

pub fn hash_text_style(s: &TextStyle) -> u64 {
    let mut h = FxHasher::default();
    h.write_u32(s.font_size.to_bits());
    hash_paint(&s.paint, &mut h);
    hash_opt_shadow(s.text_shadow.cast().as_ref(), &mut h);
    h.finish()
}

pub fn hash_path_style(s: &PathStyle) -> u64 {
    let mut h = FxHasher::default();
    hash_opt_paint(s.fill.as_ref(), &mut h);
    hash_opt_stroke(s.stroke.as_ref(), &mut h);
    hash_opt_shadow(s.shadow.as_ref(), &mut h);
    h.write_u8(match s.fill_rule {
        FillRule::Winding => 0,
        FillRule::EvenOdd => 1,
    });
    h.finish()
}

/// Hashes a span's overrides. Every field, unlike [`hash_text_style`], because a span exists precisely to
/// differ in one of them: a bold range and a plain one over identical text must not hash alike.
pub fn hash_declared(d: &Declared) -> u64 {
    let mut h = FxHasher::default();
    match &d.font_family {
        None => h.write_u8(0),
        Some(FontFamily::SansSerif) => h.write_u8(1),
        Some(FontFamily::Named(name)) => {
            h.write_u8(2);
            h.write(name.as_bytes());
        }
    }
    hash_opt_f32(d.font_size, &mut h);
    hash_opt_paint(d.paint.as_ref(), &mut h);
    match d.weight {
        None => h.write_u8(0),
        Some(w) => {
            h.write_u8(1);
            h.write_u16(w);
        }
    }
    h.write_u8(match d.italic {
        None => 0,
        Some(false) => 1,
        Some(true) => 2,
    });
    hash_opt_f32(d.letter_spacing, &mut h);
    h.write_u8(match d.raster {
        None => 0,
        Some(GlyphRaster::Smooth) => 1,
        Some(GlyphRaster::Pixel) => 2,
    });
    h.finish()
}

fn hash_opt_f32(v: Option<f32>, h: &mut FxHasher) {
    match v {
        None => h.write_u8(0),
        Some(v) => {
            h.write_u8(1);
            h.write_u32(v.to_bits());
        }
    }
}

fn hash_opt_paint(p: Option<&Paint>, h: &mut FxHasher) {
    match p {
        None => h.write_u8(0),
        Some(paint) => {
            h.write_u8(1);
            hash_paint(paint, h);
        }
    }
}

fn hash_paint(p: &Paint, h: &mut FxHasher) {
    match p {
        Paint::Solid(c) => {
            h.write_u8(0);
            h.write_u32(c.r.to_bits());
            h.write_u32(c.g.to_bits());
            h.write_u32(c.b.to_bits());
            h.write_u32(c.a.to_bits());
        }
        Paint::Gradient(g) => {
            h.write_u8(1);
            hash_gradient(g, h);
        }
    }
}

fn hash_gradient(g: &Gradient, h: &mut FxHasher) {
    match g.kind {
        GradientKind::Linear { start, end } => {
            h.write_u8(0);
            h.write_u32(start.x.to_bits());
            h.write_u32(start.y.to_bits());
            h.write_u32(end.x.to_bits());
            h.write_u32(end.y.to_bits());
        }
        GradientKind::Radial { center, radius } => {
            h.write_u8(1);
            h.write_u32(center.x.to_bits());
            h.write_u32(center.y.to_bits());
            h.write_u32(radius.to_bits());
        }
    }
    let active = g.stops.active();
    h.write_usize(active.len());
    for stop in active {
        h.write_u32(stop.position.to_bits());
        h.write_u32(stop.color.r.to_bits());
        h.write_u32(stop.color.g.to_bits());
        h.write_u32(stop.color.b.to_bits());
        h.write_u32(stop.color.a.to_bits());
    }
}

fn hash_opt_stroke(s: Option<&Stroke>, h: &mut FxHasher) {
    match s {
        None => h.write_u8(0),
        Some(stroke) => {
            h.write_u8(1);
            hash_paint(&stroke.paint, h);
            h.write_u32(stroke.width.to_bits());
            h.write_u8(stroke.cap as u8);
            h.write_u8(stroke.join as u8);
        }
    }
}

fn hash_opt_shadow(s: Option<&Shadow>, h: &mut FxHasher) {
    match s {
        None => h.write_u8(0),
        Some(shadow) => {
            h.write_u8(1);
            h.write_u32(shadow.offset_x.to_bits());
            h.write_u32(shadow.offset_y.to_bits());
            h.write_u32(shadow.blur_radius.to_bits());
            h.write_u32(shadow.spread.to_bits());
            h.write_u32(shadow.color.r.to_bits());
            h.write_u32(shadow.color.g.to_bits());
            h.write_u32(shadow.color.b.to_bits());
            h.write_u32(shadow.color.a.to_bits());
        }
    }
}

fn hash_border_radius(r: &BorderRadius, h: &mut FxHasher) {
    h.write_u32(r.top_left.to_bits());
    h.write_u32(r.top_right.to_bits());
    h.write_u32(r.bottom_right.to_bits());
    h.write_u32(r.bottom_left.to_bits());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Color;

    fn solid_path(shadow: Option<Shadow>) -> PathStyle {
        PathStyle {
            fill: Some(Paint::Solid(Color::rgba(1.0, 0.0, 0.0, 1.0))),
            stroke: None,
            shadow,
            fill_rule: FillRule::Winding,
        }
    }

    // `SvgData::id` is derived from this hasher, and renderer-assets used to carry a copy that skipped the shadow — two styles differing only there shared an id, so a cache could hand back the wrong display list.
    #[test]
    fn hash_path_style_distinguishes_a_shadow() {
        let shadow = Shadow::new(1.0, 2.0, 3.0, Color::rgba(0.0, 0.0, 0.0, 0.5));
        assert_ne!(
            hash_path_style(&solid_path(None)),
            hash_path_style(&solid_path(Some(shadow)))
        );
    }

    #[test]
    fn hash_path_style_distinguishes_two_shadows() {
        let a = Shadow::new(1.0, 2.0, 3.0, Color::rgba(0.0, 0.0, 0.0, 0.5));
        let b = Shadow::new(1.0, 2.0, 4.0, Color::rgba(0.0, 0.0, 0.0, 0.5));
        assert_ne!(
            hash_path_style(&solid_path(Some(a))),
            hash_path_style(&solid_path(Some(b)))
        );
    }

    #[test]
    fn hash_path_style_agrees_with_itself() {
        let shadow = Shadow::new(1.0, 2.0, 3.0, Color::rgba(0.0, 0.0, 0.0, 0.5));
        assert_eq!(
            hash_path_style(&solid_path(Some(shadow))),
            hash_path_style(&solid_path(Some(shadow)))
        );
    }
}
