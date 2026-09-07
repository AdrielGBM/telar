use std::sync::{Arc, Mutex};

use geometry_core::Rect;
use renderer_core::{RectStyle, ShapeStyle};

use super::*;

#[derive(Clone, Default)]
struct Recorder(Arc<Mutex<Vec<u8>>>);

impl Write for Recorder {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Recorder {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
    }
    fn clear(&self) {
        self.0.lock().unwrap().clear();
    }
}

fn renderer(sink: Recorder) -> TuiRenderer {
    TuiRenderer::new(TuiConfig::default(), Box::new(sink))
}

fn draw(r: &mut TuiRenderer, commands: &[DrawCommand]) {
    r.begin_frame(80 * 8, 24 * 16, 1.0, 0).unwrap();
    r.render_frame(commands, Some(Color::BLACK)).unwrap();
}

#[test]
fn an_identical_second_frame_writes_nothing() {
    let sink = Recorder::default();
    let mut r = renderer(sink.clone());
    let commands = [DrawCommand::Rect {
        rect: Rect::new(0.0, 0.0, 80.0, 32.0),
        style: std::sync::Arc::new(RectStyle::default().with_fill(Color::RED)),
    }];
    draw(&mut r, &commands);
    assert!(
        !sink.text().is_empty(),
        "the first frame has to paint something"
    );
    sink.clear();
    draw(&mut r, &commands);
    assert_eq!(
        sink.text(),
        "",
        "a still frame must not write to the terminal"
    );
}

/// A resize must repaint everything: the terminal cleared and reflowed what was there, so nothing the previous buffer recorded is still on screen.
#[test]
fn a_resize_repaints_in_full() {
    let sink = Recorder::default();
    let mut r = renderer(sink.clone());
    draw(&mut r, &[]);
    sink.clear();
    r.begin_frame(40 * 8, 12 * 16, 1.0, 1).unwrap();
    r.render_frame(&[], Some(Color::BLACK)).unwrap();
    assert!(
        !sink.text().is_empty(),
        "a resize repaints in full, so the flush cannot be empty"
    );
}
