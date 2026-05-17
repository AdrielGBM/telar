use crate::Event;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

#[derive(Debug, Clone)]
pub struct WindowConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: String::from("RSX App"),
            width: 800,
            height: 600,
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("platform error: {0}")]
pub struct PlatformError(pub String);

pub trait Window: HasWindowHandle + HasDisplayHandle {
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    fn request_redraw(&self);
}

pub trait EventHandler<W: Window> {
    fn on_resume(&mut self, window: &W);
    fn on_event(&mut self, event: Event, window: &W);
    fn on_redraw(&mut self, window: &W);
    fn on_suspend(&mut self) {}
}

pub trait Platform {
    type Window: Window;
    fn run<H: EventHandler<Self::Window>>(
        self,
        config: WindowConfig,
        handler: H,
    ) -> Result<(), PlatformError>;
}
