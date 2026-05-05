use rsx::{App, Color, Frame, WindowConfig, run};

struct Sandbox;

impl App for Sandbox {
    fn on_redraw(&mut self, frame: &mut Frame) {
        frame.clear(Color::BLUE);
    }
}

fn main() {
    run(WindowConfig::default(), Sandbox);
}
