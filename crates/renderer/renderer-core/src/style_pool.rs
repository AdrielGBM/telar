use std::hash::Hasher;

use rustc_hash::{FxHashMap, FxHasher};

use crate::{
    BorderRadius, FillRule, Gradient, GradientKind, Paint, PathStyle, RectStyle, Shadow, Stroke,
    TextStyle,
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct StyleHandle(pub u32);

/// Per-frame interner that maps style content to small `u32` handles so `DrawCommand` variants
/// can store an inline handle instead of an `Arc`-boxed style. Identical content within a frame
/// (and across frames while the pool persists) collapses to the same handle.
pub struct FrameStylePool {
    rect_styles: Vec<RectStyle>,
    text_styles: Vec<TextStyle>,
    path_styles: Vec<PathStyle>,
    rect_index: FxHashMap<u64, u32>,
    text_index: FxHashMap<u64, u32>,
    path_index: FxHashMap<u64, u32>,
}

impl FrameStylePool {
    pub fn new() -> Self {
        Self {
            rect_styles: Vec::new(),
            text_styles: Vec::new(),
            path_styles: Vec::new(),
            rect_index: FxHashMap::default(),
            text_index: FxHashMap::default(),
            path_index: FxHashMap::default(),
        }
    }

    pub fn clear(&mut self) {
        self.rect_styles.clear();
        self.text_styles.clear();
        self.path_styles.clear();
        self.rect_index.clear();
        self.text_index.clear();
        self.path_index.clear();
    }

    pub fn intern_rect(&mut self, style: RectStyle) -> StyleHandle {
        let hash = hash_rect_style(&style);
        if let Some(&h) = self.rect_index.get(&hash) {
            return StyleHandle(h);
        }
        let idx = self.rect_styles.len() as u32;
        self.rect_styles.push(style);
        self.rect_index.insert(hash, idx);
        StyleHandle(idx)
    }

    pub fn intern_text(&mut self, style: TextStyle) -> StyleHandle {
        let hash = hash_text_style(&style);
        if let Some(&h) = self.text_index.get(&hash) {
            return StyleHandle(h);
        }
        let idx = self.text_styles.len() as u32;
        self.text_styles.push(style);
        self.text_index.insert(hash, idx);
        StyleHandle(idx)
    }

    pub fn intern_path(&mut self, style: PathStyle) -> StyleHandle {
        let hash = hash_path_style(&style);
        if let Some(&h) = self.path_index.get(&hash) {
            return StyleHandle(h);
        }
        let idx = self.path_styles.len() as u32;
        self.path_styles.push(style);
        self.path_index.insert(hash, idx);
        StyleHandle(idx)
    }

    pub fn get_rect(&self, h: StyleHandle) -> &RectStyle {
        &self.rect_styles[h.0 as usize]
    }

    pub fn get_text(&self, h: StyleHandle) -> &TextStyle {
        &self.text_styles[h.0 as usize]
    }

    pub fn get_path(&self, h: StyleHandle) -> &PathStyle {
        &self.path_styles[h.0 as usize]
    }
}

impl Default for FrameStylePool {
    fn default() -> Self {
        Self::new()
    }
}

// Styles carry f32 fields and enums without a fixed bit layout, so they are not `bytemuck::Pod`;
// each field is hashed explicitly (f32 via `to_bits` to stay total over NaN) instead.
fn hash_rect_style(s: &RectStyle) -> u64 {
    let mut h = FxHasher::default();
    hash_opt_paint(s.fill.as_ref(), &mut h);
    hash_opt_stroke(s.stroke.as_ref(), &mut h);
    hash_opt_shadow(s.shadow.as_ref(), &mut h);
    hash_border_radius(&s.radius, &mut h);
    h.finish()
}

fn hash_text_style(s: &TextStyle) -> u64 {
    let mut h = FxHasher::default();
    h.write_u32(s.font_size.to_bits());
    hash_paint(&s.paint, &mut h);
    hash_opt_shadow(s.shadow.as_ref(), &mut h);
    h.finish()
}

fn hash_path_style(s: &PathStyle) -> u64 {
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

pub static FRAME_STYLE_POOL: std::sync::LazyLock<std::sync::Mutex<FrameStylePool>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(FrameStylePool::new()));
