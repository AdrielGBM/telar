use platform_core::WindowConfig;
use renderer_core::Color;
use ui_core::Component;

use crate::app_context::AppCtx;

pub trait App: 'static {
    fn root(&self) -> Box<dyn Component>;

    fn clear_color(&self) -> Option<Color> {
        None
    }

    fn window_config(&self) -> Option<WindowConfig> {
        None
    }

    /// Called once per frame before rendering. Use `ctx` to request redraws, change renderer backend, or access window signals.
    fn on_frame(&mut self, _ctx: &mut AppCtx) {}

    /// Hot-reload hook: serialize preserved state before this app's dylib is swapped out. Only the dylib-backed `HotApp` overrides this.
    #[doc(hidden)]
    fn hot_snapshot(&self) -> Option<String> {
        None
    }

    /// Hot-reload hook: hand a snapshot from the previous dylib to this app's dylib before its tree mounts.
    #[doc(hidden)]
    fn hot_restore(&self, _blob: &str) {}
}
