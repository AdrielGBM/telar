use geometry_core::{Point, Rect, Transform};

use crate::DrawCommand;

/// Maps clip rect `r` (in the currently-active transform's local space) to window space — the axis-aligned bounds of its four mapped corners. Widgets emit clip rects in their own local space, but the renderer clips in window space, so a clip must be mapped through the active cumulative matrix (scroll/layout translations) to compose correctly.
pub fn transform_clip_rect(m: [f32; 6], r: Rect) -> Rect {
    let [a, b, c, d, e, f] = m;
    let map = |x: f32, y: f32| (a * x + c * y + e, b * x + d * y + f);
    let corners = [
        map(r.x, r.y),
        map(r.x + r.width, r.y),
        map(r.x, r.y + r.height),
        map(r.x + r.width, r.y + r.height),
    ];
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for (x, y) in corners {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    Rect {
        x: min_x,
        y: min_y,
        width: max_x - min_x,
        height: max_y - min_y,
    }
}

/// Flat draw state tracking clips and transforms. Note: `PushLayer` and `PopLayer` commands are intentionally not tracked here; layers are managed outside this struct by the caller.
pub struct DrawState {
    clip_stack: Vec<Rect>,
    transform_stack: Vec<[f32; 6]>,
    pub cumulative_matrix: [f32; 6],
}

impl DrawState {
    pub fn new() -> Self {
        Self {
            clip_stack: Vec::with_capacity(16),
            transform_stack: Vec::with_capacity(16),
            cumulative_matrix: Transform::IDENTITY.to_array(),
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
        self.transform_stack.push(self.cumulative_matrix);
        // Compose cumulative ∘ matrix: `a.then(b)` yields `b ∘ a`, so `matrix.then(cumulative)` maps a local point through `matrix` first, then the accumulated parent chain.
        self.cumulative_matrix = Transform::from_array(matrix)
            .then(Transform::from_array(self.cumulative_matrix))
            .to_array();
    }

    #[inline]
    pub fn pop_matrix(&mut self) {
        if let Some(prev) = self.transform_stack.pop() {
            self.cumulative_matrix = prev;
        }
    }

    #[inline]
    pub fn apply_point(&self, x: f32, y: f32) -> (f32, f32) {
        let p = Transform::from_array(self.cumulative_matrix).apply(Point::new(x, y));
        (p.x, p.y)
    }

    pub fn reset(&mut self) {
        self.clip_stack.clear();
        self.transform_stack.clear();
        self.cumulative_matrix = Transform::IDENTITY.to_array();
    }
}

impl Default for DrawState {
    fn default() -> Self {
        Self::new()
    }
}

/// Iterates `cmds` calling `f(cmd, cumulative_matrix)` for every command. PushMatrix/PopMatrix update the matrix before the callback; all other commands see the matrix that was active when they were emitted.
pub fn for_each_with_matrix<F>(cmds: &[DrawCommand], mut f: F)
where
    F: FnMut(&DrawCommand, [f32; 6]),
{
    let mut state = DrawState::new();
    for cmd in cmds {
        match cmd {
            DrawCommand::PushMatrix { matrix } => state.push_matrix(*matrix),
            DrawCommand::PopMatrix => state.pop_matrix(),
            _ => {}
        }
        f(cmd, state.cumulative_matrix);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Guards the push_matrix compose argument order (cumulative ∘ matrix) against the pre-refactor
    // hand-rolled `compose_matrix(parent, child)`; a swapped order silently corrupts nested transforms.
    #[test]
    fn push_matrix_matches_legacy_compose_order() {
        let cumulative = [2.0, 0.1, -0.2, 2.0, 10.0, 20.0];
        let matrix = [1.0, 0.5, -0.5, 1.0, 5.0, 7.0];

        // Old `compose_matrix(parent = cumulative, child = matrix)` computing parent(child(p)).
        let [a1, b1, c1, d1, e1, f1] = matrix;
        let [a2, b2, c2, d2, e2, f2] = cumulative;
        let expected = [
            a2 * a1 + c2 * b1,
            b2 * a1 + d2 * b1,
            a2 * c1 + c2 * d1,
            b2 * c1 + d2 * d1,
            a2 * e1 + c2 * f1 + e2,
            b2 * e1 + d2 * f1 + f2,
        ];

        let mut state = DrawState::new();
        state.push_matrix(cumulative);
        state.push_matrix(matrix);
        assert_eq!(state.cumulative_matrix, expected);
    }
}
