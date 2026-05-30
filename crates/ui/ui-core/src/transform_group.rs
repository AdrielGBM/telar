use platform_core::Event;
use ui_tree::{Component, EventResult, View};

use crate::pointer::dispatch_to_children;

pub struct TransformGroup {
    matrix: Box<dyn Fn() -> [f32; 6]>,
    children: Vec<Box<dyn Component>>,
}

impl TransformGroup {
    pub fn new(matrix: impl Fn() -> [f32; 6] + 'static, children: Vec<Box<dyn Component>>) -> Self {
        Self {
            matrix: Box::new(matrix),
            children,
        }
    }
}

fn inverse_transform_event(event: &Event, m: [f32; 6]) -> Option<Event> {
    let [a, b, c, d, e, f] = m;
    let det = a * d - b * c;
    if det.abs() < 1e-6 {
        return None;
    }
    let inv_a = d / det;
    let inv_b = -b / det;
    let inv_c = -c / det;
    let inv_d = a / det;
    let inv_e = (c * f - d * e) / det;
    let inv_f = (b * e - a * f) / det;
    let apply = |wx: f64, wy: f64| -> (f64, f64) {
        let lx = inv_a as f64 * wx + inv_c as f64 * wy + inv_e as f64;
        let ly = inv_b as f64 * wx + inv_d as f64 * wy + inv_f as f64;
        (lx, ly)
    };
    match event {
        Event::PointerMoved { x, y, source } => {
            let (lx, ly) = apply(*x, *y);
            Some(Event::PointerMoved {
                x: lx,
                y: ly,
                source: source.clone(),
            })
        }
        Event::PointerPressed {
            x,
            y,
            button,
            source,
        } => {
            let (lx, ly) = apply(*x, *y);
            Some(Event::PointerPressed {
                x: lx,
                y: ly,
                button: button.clone(),
                source: source.clone(),
            })
        }
        Event::PointerReleased {
            x,
            y,
            button,
            source,
        } => {
            let (lx, ly) = apply(*x, *y);
            Some(Event::PointerReleased {
                x: lx,
                y: ly,
                button: button.clone(),
                source: source.clone(),
            })
        }
        _ => None,
    }
}

impl Component for TransformGroup {
    fn view(&self) -> View {
        View::Transform {
            matrix: (self.matrix)(),
            children: self.children.iter().map(|c| c.view()).collect(),
        }
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        let m = (self.matrix)();
        let transformed = inverse_transform_event(event, m);
        let effective = transformed.as_ref().unwrap_or(event);
        dispatch_to_children(&mut self.children, effective)
    }
}
