//! The [`RenderBackend`] a terminal presents.

use std::io::Write;

use renderer_core::{
    Color, DrawCommand, RenderBackend, RendererBuild, RendererError, RendererFactory,
};

use crate::buffer::CellBuffer;
use crate::color::{ColorDepth, Rgb};
use crate::metrics::CellSize;
use crate::paint::Painter;

/// What a terminal renderer is built with.
#[derive(Clone, Debug)]
pub struct TuiConfig {
    /// How many logical pixels one cell stands for. See [`CellSize`].
    pub cell: CellSize,
    pub depth: ColorDepth,
    /// What an app's own transparent pixels are composited against. A terminal will not say what colour it
    /// is drawing on, so a frame with a translucent background needs a stated assumption; black is the one
    /// that is right for the overwhelming majority of terminals and wrong in a way that is easy to see.
    pub assumed_background: Rgb,
}

impl Default for TuiConfig {
    fn default() -> Self {
        Self {
            cell: CellSize::default(),
            depth: ColorDepth::detect(),
            assumed_background: Rgb::BLACK,
        }
    }
}

pub struct TuiRenderer {
    config: TuiConfig,
    /// What the terminal is currently showing, and what the next frame is diffed against.
    front: CellBuffer,
    back: CellBuffer,
    out: Vec<u8>,
    sink: Box<dyn Write + Send>,
}

impl TuiRenderer {
    pub fn new(config: TuiConfig, sink: Box<dyn Write + Send>) -> Self {
        Self {
            // Zero-sized, so the first frame finds no matching geometry and repaints in full — which it must:
            // what is on the screen before the first frame is the user's shell, not a buffer we filled.
            front: CellBuffer::new(0, 0, config.assumed_background),
            back: CellBuffer::new(0, 0, config.assumed_background),
            out: Vec::with_capacity(16 * 1024),
            sink,
            config,
        }
    }

    /// The grid a surface of `width`×`height` logical pixels is worth.
    fn grid(&self, width: u32, height: u32) -> (u16, u16) {
        let cols = (width as f32 / self.config.cell.width).round().max(1.0);
        let rows = (height as f32 / self.config.cell.height).round().max(1.0);
        (
            cols.min(u16::MAX as f32) as u16,
            rows.min(u16::MAX as f32) as u16,
        )
    }
}

impl RenderBackend for TuiRenderer {
    fn begin_frame(
        &mut self,
        width: u32,
        height: u32,
        _scale_factor: f32,
        _generation: u64,
    ) -> Result<(), RendererError> {
        let (cols, rows) = self.grid(width, height);
        if self.back.cols() != cols || self.back.rows() != rows {
            self.back.resize(cols, rows, self.config.assumed_background);
        }
        Ok(())
    }

    fn render_frame(
        &mut self,
        commands: &[DrawCommand],
        clear_color: Option<Color>,
    ) -> Result<(), RendererError> {
        let base = match clear_color {
            Some(c) if c.a > 0.0 => self.config.assumed_background.under(c),
            _ => self.config.assumed_background,
        };
        self.back.clear(base);
        Painter::new(&mut self.back, self.config.cell).paint(commands);

        self.out.clear();
        self.back
            .diff_into(&self.front, self.config.depth, &mut self.out);
        if !self.out.is_empty() {
            self.sink
                .write_all(&self.out)
                .and_then(|()| self.sink.flush())
                .map_err(|e| RendererError::Backend(e.to_string()))?;
        }
        std::mem::swap(&mut self.front, &mut self.back);
        Ok(())
    }

    /// The terminal has no device pixels to scale into: a cell is a cell whatever the font size, and the
    /// window reports its size in the same logical units layout works in. Claiming the scale here keeps the
    /// frame pipeline from multiplying every command by a factor that means nothing.
    fn applies_scale_factor(&self) -> bool {
        true
    }
}

/// Builds a [`TuiRenderer`] for whatever window the terminal platform brings.
pub struct TuiRendererFactory {
    config: TuiConfig,
}

impl TuiRendererFactory {
    pub fn new(config: TuiConfig) -> Self {
        Self { config }
    }
}

impl Default for TuiRendererFactory {
    fn default() -> Self {
        Self::new(TuiConfig::default())
    }
}

impl<W: 'static> RendererFactory<W> for TuiRendererFactory {
    fn build(
        &self,
        _window: &W,
        _build: RendererBuild<'_>,
    ) -> Result<Box<dyn RenderBackend + Send>, RendererError> {
        Ok(Box::new(TuiRenderer::new(
            self.config.clone(),
            Box::new(std::io::stdout()),
        )))
    }

    /// The terminal draws the characters; this renderer never opens a font file.
    fn shapes_text(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use geometry_core::Rect;
    use renderer_core::{RectStyle, ShapeStyle};

    use super::*;

    #[derive(Clone, Default)]
    struct Recorder(Arc<Mutex<Vec<u8>>>);

    impl Write for Recorder {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Recorder {
        fn text(&self) -> String {
            String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
        }
        fn clear(&self) {
            self.0.lock().unwrap().clear();
        }
    }

    fn renderer(sink: Recorder) -> TuiRenderer {
        TuiRenderer::new(TuiConfig::default(), Box::new(sink))
    }

    fn draw(r: &mut TuiRenderer, commands: &[DrawCommand]) {
        r.begin_frame(80 * 8, 24 * 16, 1.0, 0).unwrap();
        r.render_frame(commands, Some(Color::BLACK)).unwrap();
    }

    #[test]
    fn an_identical_second_frame_writes_nothing() {
        let sink = Recorder::default();
        let mut r = renderer(sink.clone());
        let commands = [DrawCommand::Rect {
            rect: Rect::new(0.0, 0.0, 80.0, 32.0),
            style: std::sync::Arc::new(RectStyle::default().with_fill(Color::RED)),
        }];
        draw(&mut r, &commands);
        assert!(!sink.text().is_empty());
        sink.clear();
        draw(&mut r, &commands);
        assert_eq!(
            sink.text(),
            "",
            "a still frame must not write to the terminal"
        );
    }

    /// A resize must repaint everything: the terminal cleared and reflowed what was there, so nothing
    /// the previous buffer recorded is still on screen.
    #[test]
    fn a_resize_repaints_in_full() {
        let sink = Recorder::default();
        let mut r = renderer(sink.clone());
        draw(&mut r, &[]);
        sink.clear();
        r.begin_frame(40 * 8, 12 * 16, 1.0, 1).unwrap();
        r.render_frame(&[], Some(Color::BLACK)).unwrap();
        assert!(!sink.text().is_empty());
    }
}
