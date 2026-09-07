//! The clip and matrix stacks a backend carries while replaying commands.

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

    /// How much the accumulated matrix scales what is drawn under it.
    ///
    /// The square root of the determinant, so a rotation counts as no scale at all and a squash counts as the average of its two axes. Text is laid out at a size and not stretched from one, so one number is what it can be given.
    #[inline]
    pub fn scale(&self) -> f32 {
        let [a, b, c, d, _, _] = self.cumulative_matrix;
        (a * d - b * c).abs().sqrt()
    }

    /// Pushes `rect` as a clip, intersected with whatever is already clipping, and returns the effective scissor.
    ///
    /// A child clip that does *not* meet its parent clips away to nothing, and that case is worth spelling out because getting it wrong is invisible in every test that does not scroll: `intersect` answers `None` for two rects that do not overlap, and falling back to `rect` there hands the child its **own** box as the scissor — outside everything that contains it. A widget that emits a clip of its own (an image does, for a `Cover` overflow or a corner radius) therefore escaped the scroll area it lived in the moment it scrolled out of view: it went on being drawn at its true position, outside the panel, sliding as the content scrolled, and only looked right once its own box fell back inside the viewport. Nothing *without* a clip of its own could show the bug, which is why the text and boxes beside it in the same list were always clipped correctly.
    #[inline]
    pub fn push_clip(&mut self, rect: Rect) -> Rect {
        // At the parent's origin rather than the child's: an empty clip has no position worth keeping, and a backend that cannot express one — wgpu rejects an empty scissor, so it rounds up to 1×1 — then draws that pixel somewhere it was already allowed to, instead of leaving a stray dot outside the panel.
        let effective = match self.clip_stack.last() {
            Some(&current) => current
                .intersect(rect)
                .unwrap_or_else(|| Rect::new(current.x, current.y, 0.0, 0.0)),
            None => rect,
        };
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
#[path = "draw_state_test.rs"]
mod tests;
