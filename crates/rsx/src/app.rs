use renderer_core::Color;
use ui_core::Component;

use crate::app_context::AppCtx;

pub trait App: 'static {
    fn root(&self) -> Box<dyn Component>;

    fn clear_color(&self) -> Option<Color> {
        None
    }

    /// Called once per frame before rendering. Use `ctx` to request redraws, change renderer backend, or access window signals.
    fn on_frame(&mut self, _ctx: &mut AppCtx) {}
}
