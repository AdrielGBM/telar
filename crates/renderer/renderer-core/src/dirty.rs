//! Diffing two frames into the regions that actually changed, and recognising a scroll as a blit.

use std::ops::ControlFlow;

use geometry_core::Rect;
use smallvec::{SmallVec, smallvec};

use crate::align::{Aligner, Step};
use crate::culling::PaintBounds;
use crate::{
    DrawCommand, DrawState, GradientKind, Paint, blur_padding, blur_sigma, transform_clip_rect,
};

/// Inline capacity for the dirty-rect list. Beyond this the rects are collapsed into a single union (see MAX_DIRTY_RECTS).
pub type DirtyRects = SmallVec<[Rect; 8]>;

/// Above this count we stop tracking individual disjoint regions and fall back to a single union rect, keeping the per-frame work bounded.
const MAX_DIRTY_RECTS: usize = 4;

/// Two rects that touch or overlap (within `slop` pixels) should be merged so the dirty list stays small and the skip test stays cheap.
fn rects_adjacent_or_overlapping(a: Rect, b: Rect, slop: f32) -> bool {
    a.x <= b.x + b.width + slop
        && b.x <= a.x + a.width + slop
        && a.y <= b.y + b.height + slop
        && b.y <= a.y + a.height + slop
}

// An adjacent or overlapping rect is unioned in, which can cascade. Past `MAX_DIRTY_RECTS` distinct regions the list collapses to a single union, to bound growth.
fn push_dirty_rect(rects: &mut DirtyRects, r: Rect) {
    // Merge slop in pixels: regions separated by a thin gap are cheaper to repaint as one than to track separately.
    const SLOP: f32 = 1.0;
    if let Some(idx) = rects
        .iter()
        .position(|e| rects_adjacent_or_overlapping(*e, r, SLOP))
    {
        let mut merged = rects[idx].union(r);
        rects.swap_remove(idx);
        // The merged rect may now touch other entries; keep folding until it is disjoint from all of them.
        let mut i = 0;
        while i < rects.len() {
            if rects_adjacent_or_overlapping(rects[i], merged, SLOP) {
                merged = rects[i].union(merged);
                rects.swap_remove(i);
            } else {
                i += 1;
            }
        }
        rects.push(merged);
    } else {
        rects.push(r);
    }

    if rects.len() > MAX_DIRTY_RECTS {
        let union = rects
            .iter()
            .copied()
            .reduce(Rect::union)
            .expect("non-empty");
        *rects = smallvec![union];
    }
}

pub struct ScrollBlit {
    pub scroll_clip: Rect,
    /// Horizontal pixel shift (negative = content moved left = scroll right). Zero for Y-only scrolls.
    pub delta_x: i32,
    /// Vertical pixel shift (negative = content moved up = scroll down). Zero for X-only scrolls.
    pub delta_y: i32,
    /// The strip of newly exposed pixels that must be re-rendered (horizontal band for Y scrolls, vertical band for X scrolls).
    pub exposed_band: Rect,
    /// Everything else to repaint once the clip's pixels have moved: what changed outside the scrolled content, where it was and where it is, and whatever else is drawn inside the clip, where it is and where its moved pixels landed.
    pub extra_dirty: DirtyRects,
}

impl ScrollBlit {
    /// Whether the shift is a whole number of device pixels at `scale`, which a backend scaling logical commands itself has to know before it blits.
    pub fn is_whole_pixels_at(&self, scale: f32) -> bool {
        is_whole(self.delta_x as f32 * scale) && is_whole(self.delta_y as f32 * scale)
    }
}

// A shift snapped to whole pixels still carries float error once composed with fractional offsets or scaled.
fn is_whole(value: f32) -> bool {
    (value - value.round()).abs() <= 0.01
}

fn inflate(rect: Rect, by: f32) -> Rect {
    Rect::new(
        rect.x - by,
        rect.y - by,
        rect.width + by * 2.0,
        rect.height + by * 2.0,
    )
}

fn within(clip: Option<Rect>, rect: Rect) -> Option<Rect> {
    clip.map_or(Some(rect), |clip| clip.intersect(rect))
}

fn covers(outer: Rect, inner: Rect) -> bool {
    outer.x <= inner.x
        && outer.y <= inner.y
        && outer.x + outer.width >= inner.x + inner.width
        && outer.y + outer.height >= inner.y + inner.height
}

/// A paint whose pixels do not depend on where they land, so a blit that moves them around inside `region` moves nothing anybody can see.
#[derive(Clone, Copy)]
struct Invariance {
    /// The part of the paint that is covered evenly. Past it the paint has an edge, and an edge does move.
    region: Rect,
    /// Whether a horizontal shift leaves it looking the same.
    horizontal: bool,
    /// Whether a vertical shift does.
    vertical: bool,
}

impl Invariance {
    fn clipped(self, clip: Option<Rect>) -> Option<Self> {
        Some(Self {
            region: within(clip, self.region)?,
            ..self
        })
    }

    /// Whether blitting `scroll` leaves this paint's pixels where they belong: it covers the clip whole, and the shift is along an axis it does not vary on.
    fn absorbs(&self, scroll: &Scroll) -> bool {
        (scroll.delta_x == 0 || self.horizontal)
            && (scroll.delta_y == 0 || self.vertical)
            && covers(self.region, scroll.clip)
    }
}

/// What a command leaves unchanged under a shift, for the paints where that is provable.
///
/// Deliberately narrow: only a box, only a fill that is a solid colour or a linear gradient along one axis, and only the part of the box that fill covers evenly — a corner radius and a border each put an edge inside the bounds. Text, pictures, paths, shadows and a gradient with two directions are all left out, because a paint admitted here that does vary leaves stale pixels inside the clip that nothing will ever repaint.
fn invariance(cmd: &DrawCommand, matrix: [f32; 6]) -> Option<Invariance> {
    let DrawCommand::Rect { rect, style } = cmd else {
        return None;
    };
    // A rotation or a skew would carry a gradient's axis onto the other one, and a shadow ramps across ground the fill does not cover.
    if matrix[1] != 0.0 || matrix[2] != 0.0 || style.shadow.is_some() {
        return None;
    }
    let (horizontal, vertical) = match style.fill? {
        Paint::Solid(_) => (true, true),
        Paint::Gradient(gradient) => match gradient.kind {
            GradientKind::Linear { start, end } => (
                start.x == end.x && start.y != end.y,
                start.y == end.y && start.x != end.x,
            ),
            GradientKind::Radial { .. } => return None,
        },
    };
    if !(horizontal || vertical) {
        return None;
    }
    let radius = style.radius;
    let corners = [
        radius.top_left,
        radius.top_right,
        radius.bottom_right,
        radius.bottom_left,
    ];
    let border = style
        .painted_border()
        .map_or([0.0; 4], |(_, widths)| widths);
    let inset = corners.into_iter().chain(border).fold(0.0, f32::max);
    let even = Rect::new(
        rect.x + inset,
        rect.y + inset,
        rect.width - inset * 2.0,
        rect.height - inset * 2.0,
    );
    (even.width > 0.0 && even.height > 0.0).then(|| Invariance {
        region: transform_clip_rect(matrix, even),
        horizontal,
        vertical,
    })
}

struct ClipScope {
    damaged: bool,
}

struct LayerScope {
    damaged: bool,
    backdrop_blur: f32,
    parent_clip: Option<Rect>,
}

enum Covered {
    Nothing,
    Region(Rect),
    Backdrop(Rect),
    // A backdrop blur with nothing drawn in it is sized to the whole surface by both backends.
    Surface,
}

struct Closed {
    covered: Covered,
    damaged: bool,
}

impl Closed {
    fn backdrop(&self) -> Option<Rect> {
        match self.covered {
            Covered::Backdrop(rect) => Some(rect),
            _ => None,
        }
    }

    fn samples_backdrop(&self) -> bool {
        matches!(self.covered, Covered::Backdrop(_) | Covered::Surface)
    }
}

struct Visit {
    paint: Option<Rect>,
    /// Set when the paint would survive a blit moving it; see [`Invariance`].
    invariance: Option<Invariance>,
    matrix: [f32; 6],
    closed: Option<Closed>,
}

#[derive(Default)]
struct Replay {
    state: DrawState,
    clips: PaintBounds<ClipScope>,
    layers: PaintBounds<LayerScope>,
    matrices: usize,
}

impl Replay {
    fn reset(&mut self) {
        self.state.reset();
        self.clips.clear();
        self.layers.clear();
        self.matrices = 0;
    }

    fn visit(
        &mut self,
        cmd: &DrawCommand,
        visual_rect: &impl Fn(&DrawCommand, [f32; 6]) -> Option<Rect>,
    ) -> Visit {
        let mut closed = None;
        match cmd {
            DrawCommand::PushMatrix { matrix } => {
                self.state.push_matrix(*matrix);
                self.matrices += 1;
            }
            DrawCommand::PopMatrix => {
                self.state.pop_matrix();
                self.matrices = self.matrices.saturating_sub(1);
            }
            DrawCommand::PushClip { rect, .. } => {
                self.state
                    .push_clip(transform_clip_rect(self.state.cumulative_matrix, *rect));
                self.clips.open(ClipScope { damaged: false });
            }
            DrawCommand::PopClip => {
                closed = self.close_clip();
                self.state.pop_clip();
            }
            DrawCommand::PushLayer { backdrop_blur, .. } => self.layers.open(LayerScope {
                damaged: false,
                backdrop_blur: *backdrop_blur,
                parent_clip: self.state.current_clip(),
            }),
            DrawCommand::PopLayer => closed = self.close_layer(),
            _ => {}
        }
        let matrix = self.state.cumulative_matrix;
        let paint =
            visual_rect(cmd, matrix).and_then(|rect| within(self.state.current_clip(), rect));
        if let Some(rect) = paint {
            self.include(rect);
        }
        Visit {
            paint,
            invariance: invariance(cmd, matrix)
                .and_then(|it| it.clipped(self.state.current_clip())),
            matrix,
            closed,
        }
    }

    fn include(&mut self, rect: Rect) {
        self.clips.include(rect);
        self.layers.include(rect);
    }

    // Marking a scope here means closing it later damages everything it covers.
    fn damage_opened(&mut self, cmd: &DrawCommand) {
        match cmd {
            DrawCommand::PushClip { .. } => {
                if let Some(scope) = self.clips.innermost() {
                    scope.damaged = true;
                }
            }
            DrawCommand::PushLayer { .. } => {
                if let Some(scope) = self.layers.innermost() {
                    scope.damaged = true;
                }
            }
            _ => {}
        }
    }

    fn close_clip(&mut self) -> Option<Closed> {
        let (scope, bounds) = self.clips.close()?;
        if let Some(rect) = bounds {
            self.include(rect);
        }
        Some(Closed {
            covered: bounds.map_or(Covered::Nothing, Covered::Region),
            damaged: scope.damaged,
        })
    }

    fn close_layer(&mut self) -> Option<Closed> {
        let (scope, bounds) = self.layers.close()?;
        let covered = match bounds {
            Some(content) if scope.backdrop_blur > 0.0 => {
                let margin = blur_padding(blur_sigma(scope.backdrop_blur)) as f32;
                within(scope.parent_clip, inflate(content, margin))
                    .map_or(Covered::Nothing, Covered::Backdrop)
            }
            Some(content) => Covered::Region(content),
            None if scope.backdrop_blur > 0.0 => Covered::Surface,
            None => Covered::Nothing,
        };
        if let Covered::Region(rect) | Covered::Backdrop(rect) = covered {
            self.include(rect);
        }
        Some(Closed {
            covered,
            damaged: scope.damaged,
        })
    }

    // A list may end with clips or layers still open; they close at its end.
    fn close_open_scopes<B>(
        &mut self,
        mut visit: impl FnMut(Visit) -> ControlFlow<B>,
    ) -> ControlFlow<B> {
        while let Some(closed) = self.close_clip() {
            visit(self.closing(closed))?;
        }
        while let Some(closed) = self.close_layer() {
            visit(self.closing(closed))?;
        }
        ControlFlow::Continue(())
    }

    fn closing(&self, closed: Closed) -> Visit {
        Visit {
            paint: None,
            invariance: None,
            matrix: self.state.cumulative_matrix,
            closed: Some(closed),
        }
    }
}

#[derive(Default)]
struct Damage {
    rects: DirtyRects,
    // An unchanged backdrop repaints only once something beneath or inside it does.
    backdrops: SmallVec<[Rect; 4]>,
    samples_surface: bool,
}

impl Damage {
    fn add(&mut self, rect: Option<Rect>) {
        if let Some(rect) = rect {
            push_dirty_rect(&mut self.rects, rect);
        }
    }

    fn settle(&mut self, closed: &Option<Closed>) -> ControlFlow<()> {
        let Some(closed) = closed else {
            return ControlFlow::Continue(());
        };
        match closed.covered {
            Covered::Surface if closed.damaged => return ControlFlow::Break(()),
            Covered::Surface => self.samples_surface = true,
            Covered::Region(rect) | Covered::Backdrop(rect) if closed.damaged => {
                push_dirty_rect(&mut self.rects, rect)
            }
            Covered::Backdrop(rect) => self.backdrops.push(rect),
            Covered::Region(_) | Covered::Nothing => {}
        }
        ControlFlow::Continue(())
    }

    // A backdrop blur samples `repainted` too, so it can turn a pending backdrop into damage.
    fn finish(mut self, repainted: &[Rect]) -> Option<DirtyRects> {
        loop {
            let before = self.backdrops.len();
            let rects = &mut self.rects;
            self.backdrops.retain(|backdrop| {
                let reached = rects
                    .iter()
                    .chain(repainted)
                    .any(|rect| rect.overlaps(*backdrop));
                if reached {
                    push_dirty_rect(rects, *backdrop);
                }
                !reached
            });
            if self.backdrops.len() == before {
                break;
            }
        }
        let untouched = self.rects.is_empty() && repainted.is_empty();
        (!self.samples_surface || untouched).then_some(self.rects)
    }
}

/// One frame's side of something drawn outside the scrolled content: the pixels a blit of the clip would carry along with it, and whether carrying them would show.
#[derive(Clone, Copy)]
struct Painted {
    rect: Option<Rect>,
    invariance: Option<Invariance>,
}

impl Painted {
    fn of(visit: Option<&Visit>) -> Self {
        Self {
            rect: visit.and_then(|visit| visit.paint),
            invariance: visit.and_then(|visit| visit.invariance),
        }
    }

    /// Pixels no blit can be trusted with: a backdrop blur resamples whatever ends up beneath it.
    fn resampled(rect: Option<Rect>) -> Self {
        Self {
            rect,
            invariance: None,
        }
    }

    fn absorbed(&self, scroll: &Scroll) -> bool {
        match (self.rect, self.invariance) {
            (None, _) => true,
            (Some(_), Some(invariance)) => invariance.absorbs(scroll),
            (Some(_), None) => false,
        }
    }
}

#[derive(Clone, Copy)]
struct Scroll {
    clip: Rect,
    delta_x: i32,
    delta_y: i32,
    depth: usize,
}

impl Scroll {
    fn between(
        new_cmd: &DrawCommand,
        old_cmd: &DrawCommand,
        new: &Replay,
        old: &Replay,
    ) -> Option<Self> {
        let (DrawCommand::PushMatrix { .. }, DrawCommand::PushMatrix { .. }) = (new_cmd, old_cmd)
        else {
            return None;
        };
        if new.layers.depth() > 0 || old.layers.depth() > 0 {
            return None;
        }
        let clip = new.state.current_clip()?;
        if old.state.current_clip() != Some(clip) {
            return None;
        }
        let (n, o) = (new.state.cumulative_matrix, old.state.cumulative_matrix);
        if n[..4] != o[..4] {
            return None;
        }
        let (dx, dy) = (n[4] - o[4], n[5] - o[5]);
        if !is_whole(dx) || !is_whole(dy) {
            return None;
        }
        let (delta_x, delta_y) = (dx.round() as i32, dy.round() as i32);
        let one_axis = (delta_x == 0) != (delta_y == 0);
        let saves_something = (delta_x.unsigned_abs() as f32) < clip.width
            && (delta_y.unsigned_abs() as f32) < clip.height;
        (one_axis && saves_something).then_some(Self {
            clip,
            delta_x,
            delta_y,
            depth: new.matrices,
        })
    }

    fn exposed_band(&self) -> Rect {
        let clip = self.clip;
        if self.delta_x < 0 {
            let band = (-self.delta_x) as f32;
            Rect::new(clip.x + clip.width - band, clip.y, band, clip.height)
        } else if self.delta_x > 0 {
            Rect::new(clip.x, clip.y, self.delta_x as f32, clip.height)
        } else if self.delta_y < 0 {
            let band = (-self.delta_y) as f32;
            Rect::new(clip.x, clip.y + clip.height - band, clip.width, band)
        } else {
            Rect::new(clip.x, clip.y, clip.width, self.delta_y as f32)
        }
    }

    fn displaced(&self, new: Painted, old: Painted) -> Option<Rect> {
        // A panel background under a scrolling list is the whole clip's worth of repaint this saves: its pixels are carried along like everything else inside the clip, and land looking exactly as they did.
        if new.absorbed(self) && old.absorbed(self) {
            return None;
        }
        let (dx, dy) = (self.delta_x as f32, self.delta_y as f32);
        let now = new.rect.and_then(|rect| self.clip.intersect(rect));
        let ghost = old
            .rect
            .and_then(|rect| self.clip.intersect(rect))
            .and_then(|rect| {
                self.clip
                    .intersect(Rect::new(rect.x + dx, rect.y + dy, rect.width, rect.height))
            });
        match (now, ghost) {
            (Some(now), Some(ghost)) => Some(now.union(ghost)),
            (now, ghost) => now.or(ghost),
        }
    }
}

#[derive(Clone, Copy)]
enum Search {
    Looking,
    Inside(Scroll),
    Found(Scroll),
    Refused,
}

// Both plans at once: `damage` repaints every change over the previous frame left in place; `outside` is what a blit of the first scroll found still has to repaint.
struct Walk<'a> {
    damage: Damage,
    outside: Damage,
    search: Search,
    before_scroll: &'a mut Vec<(Painted, Painted)>,
}

impl Walk<'_> {
    fn step(
        &mut self,
        new: Option<Visit>,
        old: Option<Visit>,
        same: bool,
        new_matrices: usize,
    ) -> ControlFlow<()> {
        let (new_side, old_side) = (Painted::of(new.as_ref()), Painted::of(old.as_ref()));
        let (new_paint, old_paint) = (new_side.rect, old_side.rect);
        let moved = !same
            || new_paint != old_paint
            || new.as_ref().map(|visit| visit.matrix) != old.as_ref().map(|visit| visit.matrix);
        let new_closed = new.and_then(|visit| visit.closed);
        let old_closed = old.and_then(|visit| visit.closed);
        if moved {
            self.damage.add(new_paint);
            self.damage.add(old_paint);
        }
        self.damage.settle(&new_closed)?;
        self.damage.settle(&old_closed)?;

        match self.search {
            Search::Inside(scroll) => {
                let samples = [&new_closed, &old_closed]
                    .into_iter()
                    .flatten()
                    .any(|closed| closed.samples_backdrop());
                if !same || samples {
                    self.search = Search::Refused;
                } else if new_matrices < scroll.depth {
                    for (new, old) in self.before_scroll.drain(..) {
                        self.outside.add(scroll.displaced(new, old));
                    }
                    self.search = Search::Found(scroll);
                }
                ControlFlow::Continue(())
            }
            Search::Refused => ControlFlow::Continue(()),
            Search::Looking | Search::Found(_) => {
                if moved {
                    self.outside.add(new_paint);
                    self.outside.add(old_paint);
                }
                self.outside.settle(&new_closed)?;
                self.outside.settle(&old_closed)?;
                self.displace(new_side, old_side);
                self.displace(
                    Painted::resampled(new_closed.as_ref().and_then(Closed::backdrop)),
                    Painted::resampled(old_closed.as_ref().and_then(Closed::backdrop)),
                );
                ControlFlow::Continue(())
            }
        }
    }

    fn displace(&mut self, new: Painted, old: Painted) {
        match self.search {
            Search::Looking if new.rect.is_some() || old.rect.is_some() => {
                self.before_scroll.push((new, old))
            }
            Search::Found(scroll) => self.outside.add(scroll.displaced(new, old)),
            _ => {}
        }
    }

    fn finish(self) -> FrameChange {
        let scroll = match self.search {
            Search::Found(scroll) => {
                let exposed_band = scroll.exposed_band();
                self.outside
                    .finish(&[exposed_band])
                    .map(|extra_dirty| ScrollBlit {
                        scroll_clip: scroll.clip,
                        delta_x: scroll.delta_x,
                        delta_y: scroll.delta_y,
                        exposed_band,
                        extra_dirty,
                    })
            }
            _ => None,
        };
        FrameChange {
            damage: self.damage.finish(&[]),
            scroll,
        }
    }
}

pub struct FrameChange {
    /// The regions to repaint over the previous frame left where it is: empty when nothing visible changed, and `None` only when the change reaches the whole surface, which is a backdrop blur with nothing drawn in it.
    pub damage: Option<DirtyRects>,
    /// The same change as a blit, when the content inside a clip only scrolled.
    pub scroll: Option<ScrollBlit>,
}

/// Commands are paired by the element that drew them rather than by position, so a box that appears or disappears damages only itself and whatever moved to make room; a clear-colour change or a resize isn't in the command lists, so each backend compares those itself.
#[derive(Default)]
pub struct FrameDiff {
    aligner: Aligner,
    new: Replay,
    old: Replay,
    before_scroll: Vec<(Painted, Painted)>,
}

impl FrameDiff {
    pub fn compare(
        &mut self,
        new_cmds: &[DrawCommand],
        old_cmds: &[DrawCommand],
        visual_rect: impl Fn(&DrawCommand, [f32; 6]) -> Option<Rect>,
    ) -> FrameChange {
        let Self {
            aligner,
            new,
            old,
            before_scroll,
        } = self;
        new.reset();
        old.reset();
        before_scroll.clear();
        let mut walk = Walk {
            damage: Damage::default(),
            outside: Damage::default(),
            search: Search::Looking,
            before_scroll,
        };

        let flow = aligner.align(new_cmds, old_cmds, |step| match step {
            Step::Both(i, j) => {
                let (new_cmd, old_cmd) = (&new_cmds[i], &old_cmds[j]);
                let (n, o) = (
                    new.visit(new_cmd, &visual_rect),
                    old.visit(old_cmd, &visual_rect),
                );
                let same = new_cmd == old_cmd;
                if !same {
                    if matches!(walk.search, Search::Looking)
                        && let Some(scroll) = Scroll::between(new_cmd, old_cmd, new, old)
                    {
                        walk.search = Search::Inside(scroll);
                        return ControlFlow::Continue(());
                    }
                    new.damage_opened(new_cmd);
                    old.damage_opened(old_cmd);
                }
                walk.step(Some(n), Some(o), same, new.matrices)
            }
            Step::New(i) => {
                let n = new.visit(&new_cmds[i], &visual_rect);
                new.damage_opened(&new_cmds[i]);
                walk.step(Some(n), None, false, new.matrices)
            }
            Step::Old(j) => {
                let o = old.visit(&old_cmds[j], &visual_rect);
                old.damage_opened(&old_cmds[j]);
                walk.step(None, Some(o), false, new.matrices)
            }
        });
        let unbounded = FrameChange {
            damage: None,
            scroll: None,
        };
        if flow.is_break()
            || new
                .close_open_scopes(|visit| walk.step(Some(visit), None, false, usize::MAX))
                .is_break()
            || old
                .close_open_scopes(|visit| walk.step(None, Some(visit), false, usize::MAX))
                .is_break()
        {
            return unbounded;
        }
        walk.finish()
    }
}

#[cfg(test)]
#[path = "dirty_test.rs"]
mod tests;
