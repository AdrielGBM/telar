use crate::{Event, PlatformError};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

#[derive(Debug, Clone, Default, PartialEq)]
pub enum FullscreenMode {
    #[default]
    Disabled,
    Borderless,
    Exclusive,
}

/// The pointer shape over a window. In a modeller the cursor *is* the mode indicator — whether the next
/// press orbits, resizes a panel or places a point — so the set covers the gestures a desktop app arms
/// rather than the platform's full catalogue.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Cursor {
    #[default]
    Default,
    /// Over something clickable.
    Pointer,
    /// Over a target being aimed at — placing a point, picking an entity.
    Crosshair,
    /// Over a surface that can be dragged.
    Grab,
    /// While dragging it.
    Grabbing,
    /// Over a vertical splitter (drag left/right).
    ColResize,
    /// Over a horizontal splitter (drag up/down).
    RowResize,
    /// Over a text field.
    Text,
    /// Over a target that refuses the current gesture.
    NotAllowed,
    /// While something is in flight.
    Wait,
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
    /// Begins an OS-driven interactive move (drag the window by a custom title bar). Must be called while the
    /// pointer button that started the drag is still pressed. No-op on backends without a movable top-level
    /// window (layer-shell surfaces, headless).
    fn drag_window(&self) {}
    /// Minimizes (`true`) or restores (`false`) the window. No-op where unsupported.
    fn set_minimized(&self, _minimized: bool) {}
    /// Maximizes (`true`) or restores (`false`) the window. No-op where unsupported.
    fn set_maximized(&self, _maximized: bool) {}
    /// Whether the window is currently maximized. Defaults to `false` where unsupported.
    fn is_maximized(&self) -> bool {
        false
    }
    /// Updates the OS window title. No-op where unsupported.
    fn set_title(&self, _title: &str) {}
    /// Brings the window to the front and requests input focus. No-op where unsupported or where the window
    /// manager forbids programmatic activation (e.g. some Wayland compositors).
    fn focus_window(&self) {}
    /// Sets the pointer shape over this window. No-op where unsupported (headless).
    fn set_cursor(&self, _cursor: Cursor) {}
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
    /// Rebuilds this handler's UI from its app, on the surface it is already running on.
    ///
    /// For a backend whose surfaces outlive the state they were built from — a shell whose bars are described
    /// by a config file the user edits — this is the difference between a reload and a restart: the window,
    /// its renderer and its place on screen are kept, and only the tree is built again. A handler with no tree
    /// to rebuild leaves it a no-op.
    fn remount(&mut self, _window: &W) {}
    fn new_events(&mut self) {}
    fn about_to_wait(&mut self) -> Option<std::time::Duration> {
        None
    }
    /// Whether the handler asked to close this window (e.g. a custom title-bar close button pushed
    /// [`crate::WindowCommand::Close`]). The platform polls this after each dispatch; `true` exits the
    /// window's run loop. Implementations clear the request when returning `true`. Defaults to `false`.
    fn take_exit_request(&mut self) -> bool {
        false
    }
    /// The most recently rendered frame as premultiplied RGBA8888, if this handler renders to an offscreen
    /// target. Windowed handlers present to the screen and return `None`; an offscreen backend (e.g.
    /// [`Platform`] running headless) uses this to capture pixels after driving frames. Defaults to `None`.
    fn last_frame_rgba(&self) -> Option<Vec<u8>> {
        None
    }
    /// The laid-out rects of this handler's pointer targets, in logical surface coordinates.
    ///
    /// A backend that carves a surface's input region from its content — a click-through overlay that must
    /// still take input over its own cards — needs these, and **cannot read them itself**: they live in the
    /// handler's per-surface world, which is only active inside the handler's own calls. Read from outside,
    /// the ambient world answers instead, and it is always empty — which a backend then applies as "this
    /// surface takes no input anywhere".
    ///
    /// Defaults to empty, which is the honest answer for a handler that owns no surface world.
    fn interactive_rects(&self) -> Vec<geometry_core::Rect> {
        Vec::new()
    }
}

// Lets a multi-surface runner hold heterogeneous handlers in one map — the statically-declared surfaces and
// the dynamically-opened ones (`open_surface`) built by higher layers — behind one `Box<dyn EventHandler>`
// type, without the low-level runner naming the concrete handler type.
impl<W: Window> EventHandler<W> for Box<dyn EventHandler<W>> {
    fn on_resume(&mut self, window: &W) -> bool {
        (**self).on_resume(window)
    }
    fn on_event(&mut self, event: Event, window: &W) {
        (**self).on_event(event, window)
    }
    fn on_redraw(&mut self, window: &W) {
        (**self).on_redraw(window)
    }
    fn on_suspend(&mut self) {
        (**self).on_suspend()
    }
    fn remount(&mut self, window: &W) {
        (**self).remount(window)
    }
    fn new_events(&mut self) {
        (**self).new_events()
    }
    fn about_to_wait(&mut self) -> Option<std::time::Duration> {
        (**self).about_to_wait()
    }
    fn take_exit_request(&mut self) -> bool {
        (**self).take_exit_request()
    }
    fn last_frame_rgba(&self) -> Option<Vec<u8>> {
        (**self).last_frame_rgba()
    }
    fn interactive_rects(&self) -> Vec<geometry_core::Rect> {
        (**self).interactive_rects()
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
/// The handler for each surface is produced by a `factory`, not moved in: this is the *handler-factory* shape,
/// so a factory that builds several surfaces need not clone one app. Under M3 every surface shares one UI
/// thread and one reactive runtime (per-surface isolation comes from `ui_core::Surface`), so the factory runs
/// on the UI thread and neither it nor the handler `H` must be `Send`/`Sync` — which lets a `!Send` app (one
/// holding `Rc` state) be produced by the factory.
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
        H: EventHandler<Self::Window> + 'static,
        F: Fn(SurfaceId) -> H + 'static;
}
