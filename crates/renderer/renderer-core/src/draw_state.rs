use geometry_core::Rect;

use crate::geometry::intersect_rects;

pub struct DrawState {
    clip_stack: Vec<Rect>,
    translate_stack: Vec<(f32, f32)>,
    pub cum_tx: f32,
    pub cum_ty: f32,
}

impl DrawState {
    pub fn new() -> Self {
        Self {
            clip_stack: Vec::new(),
            translate_stack: Vec::new(),
            cum_tx: 0.0,
            cum_ty: 0.0,
        }
    }

    pub fn push_clip(&mut self, rect: Rect) -> Rect {
        let effective = self
            .clip_stack
            .last()
            .and_then(|&current| intersect_rects(current, rect))
            .unwrap_or(rect);
        self.clip_stack.push(effective);
        effective
    }

    pub fn pop_clip(&mut self) -> Option<Rect> {
        self.clip_stack.pop();
        self.clip_stack.last().copied()
    }

    pub fn push_transform(&mut self, tx: f32, ty: f32) {
        self.translate_stack.push((tx, ty));
        self.cum_tx += tx;
        self.cum_ty += ty;
    }

    pub fn pop_transform(&mut self) {
        if let Some((tx, ty)) = self.translate_stack.pop() {
            self.cum_tx -= tx;
            self.cum_ty -= ty;
        }
    }
}

impl Default for DrawState {
    fn default() -> Self {
        Self::new()
    }
}
