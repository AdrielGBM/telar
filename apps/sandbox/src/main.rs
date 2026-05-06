use rsx::{App, AppContext, BorderRadius, Color, Frame, Rect, Stroke, WindowConfig};

struct Sandbox;

impl App for Sandbox {
    fn on_redraw(&mut self, frame: &mut Frame, _ctx: &mut AppContext) {
        frame.clear(Color::WHITE);

        frame.draw_rect(
            Rect::new(50.0, 50.0, 200.0, 100.0),
            Some(Color::BLUE),
            None,
            BorderRadius::all(12.0),
        );

        frame.draw_rect(
            Rect::new(300.0, 50.0, 200.0, 100.0),
            None,
            Some(Stroke::new(Color::RED, 3.0)),
            BorderRadius::all(8.0),
        );

        frame.draw_rect(
            Rect::new(50.0, 200.0, 200.0, 100.0),
            Some(Color::GREEN),
            Some(Stroke::new(Color::BLACK, 2.0)),
            BorderRadius::zero(),
        );
    }
}

fn main() {
    rsx::run!(WindowConfig::default(), Sandbox);
}
