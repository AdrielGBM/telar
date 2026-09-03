//! The [`RenderBackend`] a document presents.

use renderer_core::{
    BuiltRenderer, Color, DrawCommand, RenderBackend, RendererBuild, RendererError, RendererFactory,
};

use crate::reconcile::Reconciler;

/// Draws Telar's frames as real elements, laid out by the browser.
///
/// The rects in the stream are ignored: CSS positions the boxes from what each one *asked* layout for, which
/// is what the element carries. Taffy still runs and still computes those rects — they are what hit-testing,
/// scrolling and every anchored overlay read, and what a parity test compares the browser's answer against.
pub struct DomRenderer {
    reconciler: Reconciler,
}

impl DomRenderer {
    pub fn new(host: web_sys::HtmlElement) -> Result<Self, String> {
        Ok(Self {
            reconciler: Reconciler::new(host)?,
        })
    }
}

impl RenderBackend for DomRenderer {
    fn begin_frame(
        &mut self,
        _width: u32,
        _height: u32,
        _scale_factor: f32,
        _generation: u64,
    ) -> Result<(), RendererError> {
        Ok(())
    }

    fn render_frame(
        &mut self,
        commands: &[DrawCommand],
        _clear_color: Option<Color>,
    ) -> Result<(), RendererError> {
        self.reconciler.frame(commands);
        Ok(())
    }

    /// The browser has already applied the device pixel ratio to every CSS pixel, so a frame described in
    /// them needs no scaling — and being handed pre-scaled commands would double it.
    fn applies_scale_factor(&self) -> bool {
        true
    }
}

/// Builds a [`DomRenderer`] on a host element chosen before the app starts.
pub struct DomRendererFactory {
    host: web_sys::HtmlElement,
}

impl DomRendererFactory {
    pub fn new(host: web_sys::HtmlElement) -> Self {
        Self { host }
    }
}

impl<W: 'static> RendererFactory<W> for DomRendererFactory {
    fn build(
        &self,
        _window: &W,
        _build: RendererBuild<'_>,
    ) -> Result<BuiltRenderer, RendererError> {
        // Inline: every node it holds is a JavaScript object, which cannot leave this thread.
        DomRenderer::new(self.host.clone())
            .map(|renderer| BuiltRenderer::Inline(Box::new(renderer)))
            .map_err(RendererError::Backend)
    }

    fn shapes_text(&self) -> bool {
        false
    }
}
