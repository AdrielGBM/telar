//! [`App`]: what an application answers for.

use platform_core::{AppCtx, WindowConfig};
use renderer_core::Color;
use ui_core::Component;

/// What an application answers for: the tree it builds, the window it wants, and what it does each frame.
///
/// Four methods, one of them required. What the *runner* needs on top of this — mounting the tree, ticking motion, draining tasks, dispatching overlays — is [`AppRuntime`](crate::AppRuntime), because the answers differ for a tree behind a dylib and an application has no business restating them.
pub trait App: 'static {
    /// The root component of this application's UI.
    fn root(&self) -> Box<dyn Component>;

    /// What the surface is cleared to before the tree is drawn. `None` keeps the theme's own.
    fn clear_color(&self) -> Option<Color> {
        None
    }

    /// Replaces the window the caller configured, outright — including any `[telar.dev.window]` override. `None` keeps what the caller passed.
    fn window_config(&self) -> Option<WindowConfig> {
        None
    }

    /// Called once per frame before rendering. Use `ctx` to request a redraw, or to take a [`RedrawWaker`](crate::RedrawWaker) for a background thread.
    fn on_frame(&mut self, _ctx: &mut AppCtx) {}

    /// The OS light/dark preference changed. The theme runtime `follow_system` reads is already updated when this runs; override it to carry the change somewhere that runtime does not reach.
    ///
    /// Which in practice means across a process or an FFI boundary: a host that draws other applications' trees out of dylibs has one theme runtime per loaded library, and only the host's own is updated for it. `Event::ColorSchemeChanged` is consumed by the runner and never reaches the tree, so this is the only place an application hears about it.
    fn on_color_scheme(&self, _dark: bool) {}
}

/// Lets a caller hold applications of different types as one — [`crate::run_multi_with_platform`] driving a surface per monitor, each with its own root.
impl<A: App + ?Sized> App for Box<A> {
    fn root(&self) -> Box<dyn Component> {
        (**self).root()
    }
    fn clear_color(&self) -> Option<Color> {
        (**self).clear_color()
    }
    fn window_config(&self) -> Option<WindowConfig> {
        (**self).window_config()
    }
    fn on_frame(&mut self, ctx: &mut AppCtx) {
        (**self).on_frame(ctx)
    }
    fn on_color_scheme(&self, dark: bool) {
        (**self).on_color_scheme(dark)
    }
}
