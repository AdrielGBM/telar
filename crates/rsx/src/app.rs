use platform_core::Event;
use renderer_core::RenderBackend;

pub use renderer_core::Color;
pub use platform_core::WindowConfig;

pub struct Frame<'a> {
    pub(crate) renderer: &'a mut dyn RenderBackend,
}

impl<'a> Frame<'a> {
    pub fn clear(&mut self, color: Color) {
        self.renderer.clear(color);
    }
}

pub trait App {
    fn on_resume(&mut self) {}
    fn on_event(&mut self, _event: Event) {}
    fn on_redraw(&mut self, frame: &mut Frame);
    fn on_suspend(&mut self) {}
}
