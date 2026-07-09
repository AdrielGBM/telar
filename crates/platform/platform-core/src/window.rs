use crate::{Event, PlatformError};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

#[derive(Debug, Clone, Default, PartialEq)]
pub enum FullscreenMode {
    #[default]
    Disabled,
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
    pub is_resizable: bool,
    pub has_decorations: bool,
    pub is_transparent: bool,
    pub fullscreen: FullscreenMode,
    pub position: WindowPosition,
    pub is_always_on_top: bool,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: String::from("RSX App"),
            width: 800,
            height: 600,
            min_size: None,
            max_size: None,
            is_resizable: true,
            has_decorations: true,
            is_transparent: false,
            fullscreen: FullscreenMode::Disabled,
            position: WindowPosition::Centered,
            is_always_on_top: false,
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
    /// The OS light/dark preference, if the platform can report it: `Some(true)` = prefer dark. `None` when
    /// undetectable (e.g. X11, or a compositor without the settings portal). Defaults to `None`.
    fn prefers_dark(&self) -> Option<bool> {
        None
    }
    /// Whether this window renders to an offscreen target with no on-screen surface (its raw handles are
    /// unavailable). A handler must build a windowless renderer for it — a windowed renderer would fail to
    /// create a surface. Defaults to `false` (an on-screen window).
    fn is_offscreen(&self) -> bool {
        false
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
    /// The most recently rendered frame as premultiplied RGBA8888, if this handler renders to an offscreen
    /// target. Windowed handlers present to the screen and return `None`; an offscreen backend (e.g.
    /// [`Platform`] running headless) uses this to capture pixels after driving frames. Defaults to `None`.
    fn last_frame_rgba(&self) -> Option<Vec<u8>> {
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

/// Identifies one surface within a [`MultiSurfacePlatform`] run. Assigned by the platform (e.g. one per
/// monitor for a desktop shell). Opaque and cheap to copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SurfaceId(pub u64);

/// A platform that drives **N** independent surfaces from a single run, each with its own
/// [`EventHandler`] — the seam a multi-window app or a desktop shell (a bar/OSD/notification per monitor)
/// needs. It is separate from [`Platform`] so the single-surface contract and every existing single-window
/// entry point stay exactly as they are.
///
/// The handler for each surface is produced by a `factory`, not moved in: this is the *handler-factory* shape.
/// Each handler therefore gets its own reactive/UI tree, and a backend that runs each surface on its own
/// thread (the natural model for headless and out-of-tree Wayland backends) gets a fully isolated reactive/
/// theme/overlay/focus world per surface for free — no cross-talk. Because the factory is invoked once per
/// surface (potentially on that surface's own thread), it is `Fn(SurfaceId) -> H` and must be `Send + Sync`;
/// the produced handler `H` never has to cross a thread boundary itself.
pub trait MultiSurfacePlatform {
    type Window: Window;
    /// Runs every surface in `surfaces` (each an `(id, config)` pair), building its handler via
    /// `factory(id)`. Blocks until all surfaces have closed.
    fn run_surfaces<H, F>(
        self,
        surfaces: Vec<(SurfaceId, WindowConfig)>,
        factory: F,
    ) -> Result<(), PlatformError>
    where
        H: EventHandler<Self::Window>,
        F: Fn(SurfaceId) -> H + Send + Sync + 'static;
}
