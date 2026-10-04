//! Field-by-field hashing for the style types, which carry floats and so cannot be hashed as plain bytes.

use std::hash::{Hash, Hasher};

use rustc_hash::FxHasher;

use crate::{
    Border, BorderRadius, Declared, DecorationLength, DecorationMetric, FillRule, FontFamily,
    FontStyle, Gradient, GradientKind, LineHeight, Paint, PathStyle, Raster, RectStyle, Shadow,
    Stroke, TextCase, TextDecoration, TextShadow, TextStyle, TextWrap,
};

// Styles carry f32 fields and enums without a fixed bit layout, so they are not `bytemuck::Pod`; each field is hashed explicitly (f32 via `to_bits` to stay total over NaN) instead.
/// The content hash of a box's paint.
pub fn hash_rect_style(s: &RectStyle) -> u64 {
    let mut h = FxHasher::default();
    hash_opt_paint(s.fill.as_ref(), &mut h);
    hash_opt_border(s.border.as_ref(), &mut h);
    hash_opt_shadow(s.shadow.as_ref(), &mut h);
    hash_border_radius(&s.radius, &mut h);
    h.finish()
}

fn hash_opt_border(b: Option<&Border>, h: &mut FxHasher) {
    match b {
        None => h.write_u8(0),
        Some(border) => {
            h.write_u8(1);
            hash_paint(&border.paint, h);
            for w in border.widths {
                h.write_u32(w.to_bits());
            }
        }
    }
}

/// The content hash of a text style: everything that changes what the glyphs look like, so two frames that differ only in weight, face or an axis value do not pass for the same frame.
pub fn hash_text_style(s: &TextStyle) -> u64 {
    let mut h = FxHasher::default();
    h.write_u32(s.font_size.to_bits());
    hash_paint(&s.color, &mut h);
    hash_opt_shadow(s.text_shadow.cast().as_ref(), &mut h);
    hash_font_family(&s.font_family, &mut h);
    h.write_u16(s.font_weight);
    h.write_u8(s.font_style as u8);
    h.write_u8(s.text_align as u8);
    h.write_u32(s.clamp.max_lines().unwrap_or(0) as u32);
    h.write_u8(u8::from(s.clamp.ellipsis()));
    h.write_u32(s.line_height.factor().map_or(0, f32::to_bits));
    h.write_u32(s.letter_spacing.to_bits());
    h.write_u8(u8::from(s.raster == Raster::Pixel));
    h.write_u8(u8::from(s.text_wrap == TextWrap::NoWrap));
    h.write_u64(s.font_variations.key());
    h.write_u64(s.font_features.key());
    // Folded in only when set, so a style without either hashes as it always has.
    if s.text_case != TextCase::AsWritten {
        h.write_u8(s.text_case as u8);
        h.write(s.lang.as_deref().unwrap_or_default().as_bytes());
    }
    if s.decoration != TextDecoration::default() {
        hash_decoration(&s.decoration, &mut h);
    }
    h.finish()
}

fn hash_decoration(d: &TextDecoration, h: &mut FxHasher) {
    h.write_u8(d.line as u8);
    hash_metric(d.offset, h);
    hash_metric(d.thickness, h);
    hash_opt_paint(d.color.as_ref(), h);
}

fn hash_metric(metric: DecorationMetric, h: &mut FxHasher) {
    match metric {
        DecorationMetric::FromFont => h.write_u8(0),
        DecorationMetric::Px(px) => {
            h.write_u8(1);
            h.write_u32(px.to_bits());
        }
    }
}

fn hash_opt_decoration_length(length: Option<DecorationLength>, h: &mut FxHasher) {
    match length {
        None => h.write_u8(0),
        Some(DecorationLength::FromFont) => h.write_u8(1),
        Some(DecorationLength::Length(length)) => {
            h.write_u8(2);
            hash_opt_length(Some(length), h);
        }
    }
}

/// The content hash of a path's paint.
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

/// Tagged by variant, with `Stack` folding its members in order so two stacks of the same families in different orders (a different fallback chain) hash apart.
fn hash_font_family(family: &FontFamily, h: &mut FxHasher) {
    match family {
        FontFamily::SansSerif => h.write_u8(1),
        FontFamily::Serif => h.write_u8(2),
        FontFamily::Monospace => h.write_u8(3),
        FontFamily::SystemUi => h.write_u8(4),
        FontFamily::Cursive => h.write_u8(5),
        FontFamily::Fantasy => h.write_u8(6),
        FontFamily::Named(name) => {
            h.write_u8(7);
            h.write(name.as_bytes());
        }
        FontFamily::Stack(families) => {
            h.write_u8(8);
            h.write_u32(families.len() as u32);
            for member in families.iter() {
                hash_font_family(member, h);
            }
        }
    }
}

/// Hashes a span's overrides. Every field, unlike `hash_text_style`, because a span exists precisely to differ in one of them: a bold range and a plain one over identical text must not hash alike.
pub fn hash_declared(d: &Declared) -> u64 {
    let mut h = FxHasher::default();
    match &d.font_family {
        None => h.write_u8(0),
        Some(family) => hash_font_family(family, &mut h),
    }
    hash_opt_length(d.font_size, &mut h);
    hash_opt_paint(d.color.as_ref(), &mut h);
    match d.font_weight {
        None => h.write_u8(0),
        Some(w) => {
            h.write_u8(1);
            h.write_u16(w);
        }
    }
    h.write_u8(match d.font_style {
        None => 0,
        Some(FontStyle::Normal) => 1,
        Some(FontStyle::Italic) => 2,
        Some(FontStyle::Oblique) => 3,
    });
    match d.line_height {
        None => h.write_u8(0),
        Some(LineHeight::Natural) => h.write_u8(1),
        Some(LineHeight::Times(n)) => {
            h.write_u8(2);
            h.write_u32(n.to_bits());
        }
    }
    hash_opt_length(d.letter_spacing, &mut h);
    h.write_u8(match d.text_align {
        None => 0,
        Some(align) => 1 + align as u8,
    });
    h.write_u8(match d.text_wrap {
        None => 0,
        Some(TextWrap::Wrap) => 1,
        Some(TextWrap::NoWrap) => 2,
    });
    match d.text_shadow {
        None => h.write_u8(0),
        Some(TextShadow::None) => h.write_u8(1),
        Some(TextShadow::Cast(shadow)) => {
            h.write_u8(2);
            hash_opt_shadow(Some(&shadow), &mut h);
        }
    }
    h.write_u8(match d.raster {
        None => 0,
        Some(Raster::Smooth) => 1,
        Some(Raster::Pixel) => 2,
    });
    h.write_u64(d.font_variations.as_ref().map_or(0, |v| v.key() ^ 1));
    h.write_u64(d.font_features.as_ref().map_or(0, |f| f.key() ^ 1));
    h.write_u8(d.text_case.map_or(0, |case| 1 + case as u8));
    h.write_u8(d.decoration_line.map_or(0, |line| 1 + line as u8));
    hash_opt_decoration_length(d.decoration_offset, &mut h);
    hash_opt_decoration_length(d.decoration_thickness, &mut h);
    hash_opt_paint(d.decoration_color.as_ref(), &mut h);
    h.finish()
}

fn hash_opt_length(v: Option<crate::TextLength>, h: &mut FxHasher) {
    match v {
        None => h.write_u8(0),
        Some(length) => {
            let (kind, bits) = length.key();
            h.write_u8(1 + kind);
            h.write_u32(bits);
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
            write_stroke(stroke, h);
        }
    }
}

/// The content hash of a stroke: paint, width, cap, join and dash.
pub(crate) fn hash_stroke(s: &Stroke) -> u64 {
    let mut h = FxHasher::default();
    write_stroke(s, &mut h);
    h.finish()
}

fn write_stroke(stroke: &Stroke, h: &mut FxHasher) {
    hash_paint(&stroke.paint, h);
    h.write_u32(stroke.width.to_bits());
    h.write_u8(stroke.cap as u8);
    h.write_u8(stroke.join as u8);
    stroke.dash.hash(h);
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
#[path = "style_pool_test.rs"]
mod tests;
