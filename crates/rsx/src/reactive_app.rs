use renderer_core::Color;
use ui_core::Component;

pub trait ReactiveApp: 'static {
    fn root(&self) -> Box<dyn Component>;

    /// Optional clear color for the window background. Defaults to `None`, meaning the framework will not clear the framebuffer before drawing the component tree.
    fn clear_color(&self) -> Option<Color> {
        None
    }
}
