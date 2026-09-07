//! The [`RenderBackend`] a terminal presents.

use std::io::Write;

use renderer_core::{
    BuiltRenderer, Color, DrawCommand, RenderBackend, RendererBuild, RendererError, RendererFactory,
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
    /// What an app's own transparent pixels are composited against. A terminal will not say what colour it is drawing on, so a frame with a translucent background needs a stated assumption; black is the one that is right for the overwhelming majority of terminals and wrong in a way that is easy to see.
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

/// The terminal backend: a cell grid, and the escape sequences one frame's difference from the last becomes.
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
            // Zero-sized, so the first frame finds no matching geometry and repaints in full — which it must: what is on the screen before the first frame is the user's shell, not a buffer we filled.
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
        Painter::new(&mut self.back, self.config.cell, self.config.depth).paint(commands);

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

    /// The terminal has no device pixels to scale into: a cell is a cell whatever the font size, and the window reports its size in the same logical units layout works in. Claiming the scale here keeps the frame pipeline from multiplying every command by a factor that means nothing.
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
    ) -> Result<BuiltRenderer, RendererError> {
        Ok(BuiltRenderer::Threaded(Box::new(TuiRenderer::new(
            self.config.clone(),
            Box::new(std::io::stdout()),
        ))))
    }

    /// The terminal draws the characters; this renderer never opens a font file.
    fn shapes_text(&self) -> bool {
        false
    }
}

#[cfg(test)]
#[path = "renderer_test.rs"]
mod tests;
