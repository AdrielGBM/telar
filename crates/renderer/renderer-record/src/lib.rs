//! A render backend that draws nothing and remembers everything.
//!
//! For asserting on what an app actually drew from a test with no GPU adapter. Every other way to ask goes
//! through pixels, which answers "does this look right" but never "was the shadow emitted at all".
//!
//! It is also the in-tree proof that [`renderer_core::RendererFactory`] is a real seam: installed from outside
//! the runtime, naming neither a window system nor a surface.

use std::sync::{Arc, Mutex};

use renderer_core::{
    BuiltRenderer, Color, DrawCommand, RenderBackend, RendererBuild, RendererError, RendererFactory,
};

/// One frame as the recorder was handed it.
#[derive(Clone, Debug)]
pub struct RecordedFrame {
    pub width: u32,
    pub height: u32,
    pub scale_factor: f32,
    pub generation: u64,
    pub clear: Option<Color>,
    pub commands: Vec<DrawCommand>,
}

/// What a recorder has seen, readable from the thread that started it.
///
/// Shared and locked because the renderer is moved onto the frame pipeline's render thread as soon as it is
/// built: by the time a test looks, the backend belongs to another thread.
#[derive(Clone, Default)]
pub struct Recording(Arc<Mutex<Vec<RecordedFrame>>>);

impl Recording {
    pub fn new() -> Self {
        Self::default()
    }

    /// Every frame recorded so far, oldest first.
    pub fn frames(&self) -> Vec<RecordedFrame> {
        self.0.lock().expect("recording lock").clone()
    }

    pub fn frame_count(&self) -> usize {
        self.0.lock().expect("recording lock").len()
    }

    /// The most recent frame, which for a settled app is the one worth asserting on.
    pub fn last_frame(&self) -> Option<RecordedFrame> {
        self.0.lock().expect("recording lock").last().cloned()
    }

    pub fn clear(&self) {
        self.0.lock().expect("recording lock").clear();
    }

    /// A backend recording into this, for a caller driving a [`RenderBackend`] directly rather than installing
    /// [`RecordingFactory`].
    pub fn backend(&self) -> RecordingRenderer {
        RecordingRenderer {
            recording: self.clone(),
            frame: None,
        }
    }
}

/// Accepts every frame, presents nothing, appends to its [`Recording`].
pub struct RecordingRenderer {
    recording: Recording,
    // What `begin_frame` established, waiting for the commands `render_frame` will bring.
    frame: Option<(u32, u32, f32, u64)>,
}

impl RenderBackend for RecordingRenderer {
    fn begin_frame(
        &mut self,
        width: u32,
        height: u32,
        scale_factor: f32,
        generation: u64,
    ) -> Result<(), RendererError> {
        self.frame = Some((width, height, scale_factor, generation));
        Ok(())
    }

    fn render_frame(
        &mut self,
        commands: &[DrawCommand],
        clear_color: Option<Color>,
    ) -> Result<(), RendererError> {
        let (width, height, scale_factor, generation) = self.frame.take().ok_or_else(|| {
            RendererError::Backend("render_frame without a begin_frame".to_string())
        })?;
        self.recording
            .0
            .lock()
            .expect("recording lock")
            .push(RecordedFrame {
                width,
                height,
                scale_factor,
                generation,
                clear: clear_color,
                commands: commands.to_vec(),
            });
        Ok(())
    }

    /// Claims to apply the scale itself, so the recording holds the frame in logical pixels as the tree composed
    /// it — letting the pipeline pre-scale would make every assertion depend on the surface's DPI.
    fn applies_scale_factor(&self) -> bool {
        true
    }
}

/// Installs a [`RecordingRenderer`] on any window, since it needs nothing from one.
pub struct RecordingFactory {
    recording: Recording,
}

impl RecordingFactory {
    pub fn new(recording: Recording) -> Self {
        Self { recording }
    }
}

impl<W> RendererFactory<W> for RecordingFactory {
    fn build(
        &self,
        _window: &W,
        _build: RendererBuild<'_>,
    ) -> Result<BuiltRenderer, RendererError> {
        Ok(BuiltRenderer::Threaded(Box::new(self.recording.backend())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recorder_keeps_each_frame_it_was_handed() {
        let recording = Recording::new();
        let mut backend = recording.backend();

        backend.begin_frame(320, 240, 2.0, 7).unwrap();
        backend
            .render_frame(&[DrawCommand::PopClip], Some(Color::BLACK))
            .unwrap();

        let frame = recording.last_frame().expect("a frame was recorded");
        assert_eq!((frame.width, frame.height), (320, 240));
        assert_eq!(frame.generation, 7);
        assert_eq!(frame.commands.len(), 1);
        assert_eq!(recording.frame_count(), 1);
    }

    // Accepting a frame with no `begin_frame` would make the recording lie about the size it was drawn at.
    #[test]
    fn commands_without_a_begun_frame_are_an_error() {
        let recording = Recording::new();
        let mut backend = recording.backend();
        assert!(backend.render_frame(&[DrawCommand::PopClip], None).is_err());
        assert_eq!(recording.frame_count(), 0);
    }
}
