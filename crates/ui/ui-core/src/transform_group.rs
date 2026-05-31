use platform_core::Event;
use ui_tree::{Component, EventResult, View};

use crate::pointer::{dispatch_to_children, transform_pointer};

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

impl Component for TransformGroup {
    fn view(&self) -> View {
        View::Transform {
            matrix: (self.matrix)(),
            children: self.children.iter().map(|c| c.view()).collect(),
        }
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        let m = (self.matrix)();
        let transformed = transform_pointer(event, m);
        let effective = transformed.as_ref().unwrap_or(event);
        dispatch_to_children(&mut self.children, effective)
    }
}
