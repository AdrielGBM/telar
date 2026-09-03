//! Reading a composed frame of draw commands into a cell grid.

mod geom;
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

pub struct Painter<'a> {
    pub(crate) buf: &'a mut CellBuffer,
    pub(crate) cell: CellSize,
    pub(crate) metrics: CellMetrics,
    state: DrawState,
    /// The composed opacity of the enclosing layers. Always non-empty; `1.0` at the bottom.
    opacity: Vec<f32>,
    /// Reused across every paragraph in a frame, so wrapping allocates once per process rather than per text.
    pub(crate) lines: Vec<WrappedLine>,
}

impl<'a> Painter<'a> {
    pub fn new(buf: &'a mut CellBuffer, cell: CellSize) -> Self {
        Self {
            buf,
            cell,
            metrics: CellMetrics::new(cell),
            state: DrawState::new(),
            opacity: vec![1.0],
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
                // The radius is dropped: a cell is either inside the clip or outside it, and there is no
                // sub-cell coverage in which a rounded corner could mean anything.
                let mapped =
                    renderer_core::transform_clip_rect(self.state.cumulative_matrix, *rect);
                self.state.push_clip(mapped);
            }
            DrawCommand::PopClip => {
                self.state.pop_clip();
            }
            DrawCommand::PushLayer { opacity, .. } => {
                // `backdrop_blur` has no terminal expression and is dropped rather than approximated: a
                // wrong blur reads as a rendering fault, an absent one as a plainer surface.
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
            // Pictures need a graphics protocol, which is negotiated with the terminal rather than decided
            // here. Until that lands a picture leaves its box alone rather than filling it with a guess.
            DrawCommand::Image { .. } => {}
        }
    }

    pub(crate) fn matrix(&self) -> [f32; 6] {
        self.state.cumulative_matrix
    }

    pub(crate) fn scale(&self) -> f32 {
        self.state.scale()
    }

    pub(crate) fn alpha(&self) -> f32 {
        *self.opacity.last().unwrap_or(&1.0)
    }

    /// The cells a logical rect covers, cut to the active clip and to the buffer.
    pub(crate) fn cells_of(&self, rect: Rect) -> CellRect {
        let mapped = renderer_core::transform_clip_rect(self.state.cumulative_matrix, rect);
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

    /// Whether a cell is inside the active clip. Used by painters that walk cells one at a time rather than
    /// by rectangle — a path scanline, a line.
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

    fn faded(&self, color: Color) -> Color {
        let a = self.alpha();
        if a >= 1.0 {
            color
        } else {
            color.with_alpha(color.a * a)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use geometry_core::Rect;
    use renderer_core::{Border, BorderRadius, Color, Paint, RectStyle, ShapeStyle, TextStyle};

    use super::*;

    fn grid(cols: u16, rows: u16) -> CellBuffer {
        CellBuffer::new(cols, rows, Rgb::BLACK)
    }

    /// The characters of one row, so an assertion reads like the screen does.
    fn row(buf: &CellBuffer, row: u16) -> String {
        (0..buf.cols())
            .filter_map(|c| buf.get(c, row))
            .filter(|c| !c.attrs.contains(Attrs::WIDE_TAIL))
            .map(|c| c.glyph.as_str())
            .collect()
    }

    fn paint(buf: &mut CellBuffer, commands: &[DrawCommand]) {
        Painter::new(buf, CellSize::default()).paint(commands);
    }

    fn rect_cmd(rect: Rect, style: RectStyle) -> DrawCommand {
        DrawCommand::Rect {
            rect,
            style: Arc::new(style),
        }
    }

    fn text_cmd(text: &str, rect: Rect) -> DrawCommand {
        DrawCommand::Text {
            text: text.into(),
            spans: None,
            rect,
            style: Arc::new(TextStyle::new(14.0, Paint::Solid(Color::WHITE))),
        }
    }

    #[test]
    fn a_fill_colours_every_cell_it_covers() {
        let mut buf = grid(10, 3);
        paint(
            &mut buf,
            &[rect_cmd(
                Rect::new(0.0, 0.0, 8.0 * 4.0, 16.0 * 2.0),
                RectStyle::default().with_fill(Color::RED),
            )],
        );
        assert_eq!(buf.get(3, 1).unwrap().bg, Rgb { r: 255, g: 0, b: 0 });
        assert_eq!(
            buf.get(4, 1).unwrap().bg,
            Rgb::BLACK,
            "one column past the edge"
        );
        assert_eq!(
            buf.get(0, 2).unwrap().bg,
            Rgb::BLACK,
            "one row past the edge"
        );
    }

    #[test]
    fn a_border_draws_a_closed_frame() {
        let mut buf = grid(10, 4);
        paint(
            &mut buf,
            &[rect_cmd(
                Rect::new(0.0, 0.0, 8.0 * 5.0, 16.0 * 3.0),
                RectStyle::default().with_border(Border::uniform(Paint::Solid(Color::WHITE), 1.0)),
            )],
        );
        assert_eq!(row(&buf, 0), "┌───┐     ");
        assert_eq!(row(&buf, 1), "│   │     ");
        assert_eq!(row(&buf, 2), "└───┘     ");
    }

    #[test]
    fn a_rounded_border_uses_rounded_corners() {
        let mut buf = grid(6, 3);
        paint(
            &mut buf,
            &[rect_cmd(
                Rect::new(0.0, 0.0, 8.0 * 4.0, 16.0 * 3.0),
                RectStyle::default()
                    .with_border(Border::uniform(Paint::Solid(Color::WHITE), 1.0))
                    .with_radius(BorderRadius::all(8.0)),
            )],
        );
        assert_eq!(row(&buf, 0), "╭──╮  ");
        assert_eq!(row(&buf, 2), "╰──╯  ");
    }

    #[test]
    fn a_thick_border_uses_the_heavy_set() {
        let mut buf = grid(6, 3);
        paint(
            &mut buf,
            &[rect_cmd(
                Rect::new(0.0, 0.0, 8.0 * 4.0, 16.0 * 3.0),
                RectStyle::default().with_border(Border::uniform(Paint::Solid(Color::WHITE), 6.0)),
            )],
        );
        assert_eq!(row(&buf, 0), "┏━━┓  ");
    }

    #[test]
    fn a_border_on_one_side_only_draws_that_side() {
        let mut buf = grid(6, 3);
        paint(
            &mut buf,
            &[rect_cmd(
                Rect::new(0.0, 0.0, 8.0 * 4.0, 16.0 * 3.0),
                RectStyle::default().with_border(Border::per_side(
                    Paint::Solid(Color::WHITE),
                    0.0,
                    0.0,
                    1.0,
                    0.0,
                )),
            )],
        );
        assert_eq!(row(&buf, 0), "      ");
        assert_eq!(row(&buf, 2), "────  ");
    }

    #[test]
    fn text_starts_where_its_box_does() {
        let mut buf = grid(12, 2);
        paint(
            &mut buf,
            &[text_cmd(
                "hola",
                Rect::new(8.0 * 2.0, 16.0, 8.0 * 4.0, 16.0),
            )],
        );
        assert_eq!(row(&buf, 1), "  hola      ");
    }

    #[test]
    fn centred_text_is_centred_in_its_box() {
        let mut buf = grid(12, 1);
        let mut style = TextStyle::new(14.0, Paint::Solid(Color::WHITE));
        style.text_align = renderer_core::TextAlign::Center;
        paint(
            &mut buf,
            &[DrawCommand::Text {
                text: "ab".into(),
                spans: None,
                rect: Rect::new(0.0, 0.0, 8.0 * 8.0, 16.0),
                style: Arc::new(style),
            }],
        );
        assert_eq!(row(&buf, 0), "   ab       ");
    }

    #[test]
    fn a_clip_cuts_the_paragraph() {
        let mut buf = grid(12, 1);
        paint(
            &mut buf,
            &[
                DrawCommand::PushClip {
                    rect: Rect::new(0.0, 0.0, 8.0 * 2.0, 16.0),
                    radius: BorderRadius::zero(),
                },
                text_cmd("abcdef", Rect::new(0.0, 0.0, 8.0 * 6.0, 16.0)),
                DrawCommand::PopClip,
            ],
        );
        assert_eq!(row(&buf, 0), "ab          ");
    }

    #[test]
    fn a_matrix_moves_what_is_drawn_under_it() {
        let mut buf = grid(12, 2);
        paint(
            &mut buf,
            &[
                DrawCommand::PushMatrix {
                    matrix: [1.0, 0.0, 0.0, 1.0, 8.0 * 3.0, 16.0],
                },
                text_cmd("hi", Rect::new(0.0, 0.0, 8.0 * 2.0, 16.0)),
                DrawCommand::PopMatrix,
            ],
        );
        assert_eq!(row(&buf, 1), "   hi       ");
    }

    #[test]
    fn a_layer_fades_what_it_holds() {
        let mut buf = grid(4, 1);
        paint(
            &mut buf,
            &[
                DrawCommand::PushLayer {
                    opacity: 0.5,
                    backdrop_blur: 0.0,
                },
                rect_cmd(
                    Rect::new(0.0, 0.0, 8.0 * 4.0, 16.0),
                    RectStyle::default().with_fill(Color::WHITE),
                ),
                DrawCommand::PopLayer,
            ],
        );
        let bg = buf.get(0, 0).unwrap().bg;
        assert!(bg.r.abs_diff(128) <= 1, "got {bg:?}");
    }

    #[test]
    fn a_filled_path_covers_its_interior() {
        let mut buf = grid(8, 4);
        let path = renderer_core::PathData::new()
            .move_to(geometry_core::Point::new(0.0, 0.0))
            .line_to(geometry_core::Point::new(8.0 * 4.0, 0.0))
            .line_to(geometry_core::Point::new(8.0 * 4.0, 16.0 * 2.0))
            .line_to(geometry_core::Point::new(0.0, 16.0 * 2.0))
            .close();
        paint(
            &mut buf,
            &[DrawCommand::Path {
                data: Arc::new(path),
                style: Arc::new(renderer_core::PathStyle::default().with_fill(Color::GREEN)),
            }],
        );
        assert_eq!(buf.get(1, 1).unwrap().bg, Rgb { r: 0, g: 255, b: 0 });
        assert_eq!(buf.get(5, 1).unwrap().bg, Rgb::BLACK, "outside the path");
    }
}
