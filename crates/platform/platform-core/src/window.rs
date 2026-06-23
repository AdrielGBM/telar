use crate::{Event, PlatformError};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

#[derive(Debug, Clone, Default, PartialEq)]
pub enum FullscreenMode {
    #[default]
    None,
    Borderless,
    Exclusive,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub enum WindowPosition {
    #[default]
    Centered,
    At(i32, i32),
}

#[derive(Debug, Clone)]
pub struct WindowConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub min_size: Option<(u32, u32)>,
    pub max_size: Option<(u32, u32)>,
    pub resizable: bool,
    pub decorations: bool,
    pub transparent: bool,
    pub fullscreen: FullscreenMode,
    pub position: WindowPosition,
    pub always_on_top: bool,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: String::from("RSX App"),
            width: 800,
            height: 600,
            min_size: None,
            max_size: None,
            resizable: true,
            decorations: true,
            transparent: false,
            fullscreen: FullscreenMode::None,
            position: WindowPosition::Centered,
            always_on_top: false,
        }
    }
}

pub trait Window: HasWindowHandle + HasDisplayHandle {
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    fn request_redraw(&self);
    fn scale_factor(&self) -> f64 {
        1.0
    }
}

pub trait EventHandler<W: Window> {
    fn on_resume(&mut self, window: &W) -> bool;
    fn on_event(&mut self, event: Event, window: &W);
    fn on_redraw(&mut self, window: &W);
    fn on_suspend(&mut self) {}
    fn new_events(&mut self) {}
    fn about_to_wait(&mut self) -> Option<std::time::Duration> {
        None
    }
}

pub trait Platform {
    type Window: Window;
    fn run<H: EventHandler<Self::Window>>(
        self,
        config: WindowConfig,
        handler: H,
    ) -> Result<(), PlatformError>;
}
