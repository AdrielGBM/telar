//! Reading a composed frame of draw commands into a cell grid.

mod geom;
mod image;
mod shape;
mod text;

use geometry_core::Rect;
use renderer_core::{Color, DrawCommand, DrawState};

pub use geom::CellRect;

use crate::buffer::CellBuffer;
use crate::cell::Attrs;
use crate::color::Rgb;
use crate::metrics::{CellMetrics, CellSize};
use crate::wrap::WrappedLine;

/// Paints a command list into a cell grid, in whatever a terminal can express of it.
/// How much of a cell a fill has to claim before the glyph beneath it is gone.
///
/// Strictly more than half, so the paint has to contribute more to the cell than whatever was under it. That leaves a scrim — half-alpha by definition — dimming the page it covers instead of erasing it, which is the one case where seeing through is the whole point.
const FILL_HIDES_GLYPH_ABOVE: f32 = 0.5;

pub struct Painter<'a> {
    pub(crate) buf: &'a mut CellBuffer,
    pub(crate) cell: CellSize,
    pub(crate) metrics: CellMetrics,
    state: DrawState,
    /// The composed opacity of the enclosing layers. Always non-empty; `1.0` at the bottom.
    opacity: Vec<f32>,
    /// What the terminal can hold, which decides how far an alpha can be trusted. See [`crate::ColorDepth::compress_alpha`].
    pub(crate) depth: crate::ColorDepth,
    /// Reused across every paragraph in a frame, so wrapping allocates once per process rather than per text.
    pub(crate) lines: Vec<WrappedLine>,
}

impl<'a> Painter<'a> {
    pub fn new(buf: &'a mut CellBuffer, cell: CellSize, depth: crate::ColorDepth) -> Self {
        Self {
            buf,
            cell,
            metrics: CellMetrics::new(cell),
            state: DrawState::new(),
            opacity: vec![1.0],
            depth,
            lines: Vec::new(),
        }
    }

    pub fn paint(&mut self, commands: &[DrawCommand]) {
        for command in commands {
            self.one(command);
        }
    }

    fn one(&mut self, command: &DrawCommand) {
        match command {
            DrawCommand::PushMatrix { matrix } => self.state.push_matrix(*matrix),
            DrawCommand::PopMatrix => self.state.pop_matrix(),
            DrawCommand::PushClip { rect, .. } => {
                // The radius is dropped: a cell is either inside the clip or outside it, and there is no sub-cell coverage in which a rounded corner could mean anything.
                let mapped = renderer_core::transform_clip_rect(self.grid_matrix(), *rect);
                self.state.push_clip(mapped);
            }
            DrawCommand::PopClip => {
                self.state.pop_clip();
            }
            DrawCommand::PushLayer { opacity, .. } => {
                // `backdrop_blur` has no terminal expression and is dropped rather than approximated: a wrong blur reads as a rendering fault, an absent one as a plainer surface.
                let composed = self.alpha() * opacity.clamp(0.0, 1.0);
                self.opacity.push(composed);
            }
            DrawCommand::PopLayer => {
                if self.opacity.len() > 1 {
                    self.opacity.pop();
                }
            }
            DrawCommand::Rect { rect, style } => self.rect(*rect, style),
            DrawCommand::Text {
                text,
                spans,
                rect,
                style,
            } => self.text(text, spans.as_deref(), *rect, style),
            DrawCommand::Line { p1, p2, style } => self.line(*p1, *p2, style),
            DrawCommand::Path { data, style } => self.path(data, style),
            DrawCommand::Image { data, rect, raster } => self.image(data, *rect, *raster),
            // Structure, for a backend whose output is a document. The commands inside are already where they belong, so skipping the markers draws exactly the same grid.
            DrawCommand::PushElement { .. } | DrawCommand::PopElement => {}
        }
    }

    /// The cumulative matrix with its translation put on the cell grid.
    ///
    /// The last line of defence for the invariant the layout grid sets up: a box that is a whole number of cells only *stays* one if whatever moves it is a whole number of cells too. Snapped sizes cover what layout builds and a snapped scroll offset covers the wheel, but a matrix can come from anywhere — an easing transition, a drag, a transform an app wrote by hand — and half a cell of translation reintroduces exactly the position-dependent rounding the rest of this exists to remove.
    ///
    /// The translation only. Scale and rotation change what a cell *means* rather than where it is, and a terminal has no answer for either that rounding could improve.
    fn grid_matrix(&self) -> [f32; 6] {
        let grid = geometry_core::LayoutGrid::new(self.cell.width, self.cell.height);
        let m = self.state.cumulative_matrix;
        [
            m[0],
            m[1],
            m[2],
            m[3],
            grid.snap_pos_x(m[4]),
            grid.snap_pos_y(m[5]),
        ]
    }

    pub(crate) fn matrix(&self) -> [f32; 6] {
        self.grid_matrix()
    }

    pub(crate) fn scale(&self) -> f32 {
        self.state.scale()
    }

    pub(crate) fn alpha(&self) -> f32 {
        *self.opacity.last().unwrap_or(&1.0)
    }

    /// The cells a logical rect covers, cut to the active clip and to the buffer.
    pub(crate) fn cells_of(&self, rect: Rect) -> CellRect {
        let mapped = renderer_core::transform_clip_rect(self.grid_matrix(), rect);
        let mut r = CellRect::of(mapped, self.cell);
        if let Some(clip) = self.state.current_clip() {
            r = r.intersect(CellRect::of(clip, self.cell));
        }
        r.intersect(CellRect {
            col0: 0,
            row0: 0,
            col1: self.buf.cols() as i32,
            row1: self.buf.rows() as i32,
        })
    }

    /// Whether a cell is inside the active clip. Used by painters that walk cells one at a time rather than by rectangle — a path scanline, a line.
    pub(crate) fn clipped_in(&self, col: i32, row: i32) -> bool {
        let Some(clip) = self.state.current_clip() else {
            return col >= 0
                && row >= 0
                && col < self.buf.cols() as i32
                && row < self.buf.rows() as i32;
        };
        let c = CellRect::of(clip, self.cell).intersect(CellRect {
            col0: 0,
            row0: 0,
            col1: self.buf.cols() as i32,
            row1: self.buf.rows() as i32,
        });
        col >= c.col0 && col < c.col1 && row >= c.row0 && row < c.row1
    }

    /// Composites `color` into a cell's background, faded by the enclosing layers.
    /// A fill covering one whole cell: the background blends, and the glyph under it goes if the paint is solid enough that any other backend would have hidden it.
    ///
    /// A raster backend needs no such rule — an opaque fill covers the pixels a letter was drawn from, a translucent one blends with them. A cell holds one glyph and has no partial coverage, so painting a panel over a paragraph left the paragraph legible *through* it, and a modal, a menu and a tooltip each read as though the page behind were still in front.
    pub(crate) fn fill_cell(&mut self, col: i32, row: i32, color: Color) {
        let color = self.faded(color);
        if color.a <= 0.0 {
            return;
        }
        let (Ok(c), Ok(r)) = (u16::try_from(col), u16::try_from(row)) else {
            return;
        };
        let Some(cell) = self.buf.get_mut(c, r) else {
            return;
        };
        cell.bg = cell.bg.under(color);
        if color.a > FILL_HIDES_GLYPH_ABOVE {
            let bg = cell.bg;
            // Through `put` rather than by assigning the glyph: clearing the head of a double-width grapheme has to clear its tail as well, or the tail outlives it as an orphan the diff will never revisit.
            self.buf
                .put(c, r, crate::cell::Grapheme::SPACE, bg, Attrs::NONE);
        }
    }

    pub(crate) fn blend_bg(&mut self, col: i32, row: i32, color: Color) {
        let color = self.faded(color);
        if color.a <= 0.0 {
            return;
        }
        let (Ok(col), Ok(row)) = (u16::try_from(col), u16::try_from(row)) else {
            return;
        };
        if let Some(cell) = self.buf.get_mut(col, row) {
            cell.bg = cell.bg.under(color);
        }
    }

    /// Writes a glyph, taking its foreground from `color` and leaving the background as it is.
    pub(crate) fn put_glyph(
        &mut self,
        col: i32,
        row: i32,
        glyph: crate::cell::Grapheme,
        color: Color,
        attrs: Attrs,
    ) -> u16 {
        let color = self.faded(color);
        let (Ok(c), Ok(r)) = (u16::try_from(col), u16::try_from(row)) else {
            return 1;
        };
        let bg = self.buf.get(c, r).map(|cell| cell.bg).unwrap_or(Rgb::BLACK);
        self.buf.put(c, r, glyph, bg.under(color), attrs)
    }

    /// A colour as this surface will actually show it: the enclosing layers applied, then the palette's own floor. Both are alpha, and both have to be settled before anything blends — see [`ColorDepth::compress_alpha`].
    fn faded(&self, color: Color) -> Color {
        let layered = color.a * self.alpha();
        let shown = self.depth.compress_alpha(layered);
        if shown == color.a {
            color
        } else {
            color.with_alpha(shown)
        }
    }
}

#[cfg(test)]
#[path = "paint_test.rs"]
mod tests;
