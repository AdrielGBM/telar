use rsx::{App, AppContext, Color, Frame, WindowConfig};

struct Sandbox;

impl App for Sandbox {
    fn on_redraw(&mut self, frame: &mut Frame, _ctx: &mut AppContext) {
        frame.clear(Color::BLUE);
    }
}

fn main() {
    rsx::run!(WindowConfig::default(), Sandbox);
}
