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
mod tests {
    use super::*;

    // Guards the push_matrix compose argument order (cumulative ∘ matrix) against the pre-refactor hand-rolled `compose_matrix(parent, child)`; a swapped order silently corrupts nested transforms.
    #[test]
    fn push_matrix_matches_legacy_compose_order() {
        let cumulative = [2.0, 0.1, -0.2, 2.0, 10.0, 20.0];
        let matrix = [1.0, 0.5, -0.5, 1.0, 5.0, 7.0];

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

    /// A clip nested inside one it does not touch must clip away to nothing. The failing case is a widget that emits its own clip — an image with a `Cover` overflow or a corner radius — scrolled out of the viewport containing it: falling back to the child's own rect scissors to a box outside the parent, and the widget goes on being drawn there, over whatever the panel happens to be sitting on.
    #[test]
    fn a_clip_outside_its_parent_clips_away_to_nothing() {
        let viewport = Rect::new(0.0, 0.0, 340.0, 300.0);
        let mut state = DrawState::new();
        assert_eq!(
            state.push_clip(viewport),
            viewport,
            "the outermost clip is itself"
        );

        let escaped = state.push_clip(Rect::new(0.0, 560.0, 56.0, 56.0));
        assert_eq!(
            (escaped.width, escaped.height),
            (0.0, 0.0),
            "an image scrolled out of its panel must not be drawn at all, got {escaped:?}"
        );
        assert!(
            viewport
                .intersect(Rect::new(escaped.x, escaped.y, 1.0, 1.0))
                .is_some(),
            "and the empty clip sits inside the parent, so a backend that rounds it up to 1×1 draws that \
             pixel where it was already allowed to: {escaped:?}"
        );

        state.pop_clip();
        let visible = state.push_clip(Rect::new(0.0, 280.0, 56.0, 56.0));
        assert_eq!(visible, Rect::new(0.0, 280.0, 56.0, 20.0));

        let mut fresh = DrawState::new();
        let alone = Rect::new(12.0, 34.0, 56.0, 78.0);
        assert_eq!(fresh.push_clip(alone), alone);
    }

    /// What text is laid out at. The rect a matrix scales already grows; without one number saying by how much, the letters inside it would not — which is a box of the right size holding text of the wrong one.
    #[test]
    fn the_scale_of_a_matrix_is_what_it_makes_things() {
        let mut state = DrawState::new();
        assert_eq!(state.scale(), 1.0, "nothing scales nothing");

        state.push_matrix([2.0, 0.0, 0.0, 2.0, 0.0, 0.0]);
        assert_eq!(state.scale(), 2.0);

        state.push_matrix([3.0, 0.0, 0.0, 3.0, 0.0, 0.0]);
        assert_eq!(state.scale(), 6.0);

        state.pop_matrix();
        assert_eq!(state.scale(), 2.0);
    }

    /// A translation moves things and does not make them bigger, so text under one is laid out at the size it was written with. Every leaf in the tree sits under one of these.
    #[test]
    fn moving_something_is_not_scaling_it() {
        let mut state = DrawState::new();
        state.push_matrix([1.0, 0.0, 0.0, 1.0, 40.0, 90.0]);
        assert_eq!(state.scale(), 1.0);
    }

    /// A rotation is not a scale either, and reading one axis rather than the determinant would say it was.
    #[test]
    fn turning_something_is_not_scaling_it() {
        let mut state = DrawState::new();
        let turn = std::f32::consts::FRAC_PI_4;
        state.push_matrix([turn.cos(), turn.sin(), -turn.sin(), turn.cos(), 0.0, 0.0]);
        assert!((state.scale() - 1.0).abs() < 1e-5, "{}", state.scale());
    }
}
