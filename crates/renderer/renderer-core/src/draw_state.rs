use geometry_core::Rect;

pub const IDENTITY_MATRIX: [f32; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

// Computes outer(inner(point)) for two affine matrices [a, b, c, d, e, f].
pub fn compose_matrix(outer: [f32; 6], inner: [f32; 6]) -> [f32; 6] {
    let [a1, b1, c1, d1, e1, f1] = inner;
    let [a2, b2, c2, d2, e2, f2] = outer;
    [
        a2 * a1 + c2 * b1,
        b2 * a1 + d2 * b1,
        a2 * c1 + c2 * d1,
        b2 * c1 + d2 * d1,
        a2 * e1 + c2 * f1 + e2,
        b2 * e1 + d2 * f1 + f2,
    ]
}

pub struct DrawState {
    clip_stack: Vec<Rect>,
    transform_stack: Vec<[f32; 6]>,
    pub cum_matrix: [f32; 6],
}

impl DrawState {
    pub fn new() -> Self {
        Self {
            clip_stack: Vec::new(),
            transform_stack: Vec::new(),
            cum_matrix: IDENTITY_MATRIX,
        }
    }

    #[inline]
    pub fn push_clip(&mut self, rect: Rect) -> Rect {
        let effective = self
            .clip_stack
            .last()
            .and_then(|&current| current.intersect(rect))
            .unwrap_or(rect);
        self.clip_stack.push(effective);
        effective
    }

    #[inline]
    pub fn pop_clip(&mut self) -> Option<Rect> {
        self.clip_stack.pop();
        self.clip_stack.last().copied()
    }

    #[inline]
    pub fn current_clip(&self) -> Option<Rect> {
        self.clip_stack.last().copied()
    }

    #[inline]
    pub fn push_matrix(&mut self, matrix: [f32; 6]) {
        self.transform_stack.push(self.cum_matrix);
        self.cum_matrix = compose_matrix(self.cum_matrix, matrix);
    }

    #[inline]
    pub fn pop_matrix(&mut self) {
        if let Some(prev) = self.transform_stack.pop() {
            self.cum_matrix = prev;
        }
    }

    #[inline]
    pub fn apply_point(&self, x: f32, y: f32) -> (f32, f32) {
        crate::culling::apply_matrix(self.cum_matrix, x, y)
    }

    pub fn reset(&mut self) {
        self.clip_stack.clear();
        self.transform_stack.clear();
        self.cum_matrix = IDENTITY_MATRIX;
    }
}

impl Default for DrawState {
    fn default() -> Self {
        Self::new()
    }
}
