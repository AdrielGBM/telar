use std::hash::Hasher;

use rustc_hash::FxHasher;

use crate::{
    BorderRadius, FillRule, Gradient, GradientKind, Paint, PathStyle, RectStyle, Shadow, Stroke,
    TextStyle,
};

// Styles carry f32 fields and enums without a fixed bit layout, so they are not `bytemuck::Pod`; each field is hashed explicitly (f32 via `to_bits` to stay total over NaN) instead.
pub fn hash_rect_style(s: &RectStyle) -> u64 {
    let mut h = FxHasher::default();
    hash_opt_paint(s.fill.as_ref(), &mut h);
    hash_opt_stroke(s.stroke.as_ref(), &mut h);
    hash_opt_shadow(s.shadow.as_ref(), &mut h);
    hash_border_radius(&s.radius, &mut h);
    h.finish()
}

pub fn hash_text_style(s: &TextStyle) -> u64 {
    let mut h = FxHasher::default();
    h.write_u32(s.font_size.to_bits());
    hash_paint(&s.paint, &mut h);
    hash_opt_shadow(s.shadow.as_ref(), &mut h);
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
