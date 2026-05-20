use renderer_core::Color;
use ui_core::Component;

pub trait App: 'static {
    fn root(&self) -> Box<dyn Component>;

    fn clear_color(&self) -> Option<Color> {
        None
    }
}
