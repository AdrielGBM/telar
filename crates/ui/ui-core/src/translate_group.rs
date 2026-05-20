use platform_core::Event;
use ui_tree::{Component, EventResult, View};

use crate::pointer::{dispatch_to_children, offset_pointer};

pub struct TranslateGroup {
    tx: Box<dyn Fn() -> f32>,
    ty: Box<dyn Fn() -> f32>,
    children: Vec<Box<dyn Component>>,
}

impl TranslateGroup {
    pub fn new(
        tx: impl Fn() -> f32 + 'static,
        ty: impl Fn() -> f32 + 'static,
        children: Vec<Box<dyn Component>>,
    ) -> Self {
        Self {
            tx: Box::new(tx),
            ty: Box::new(ty),
            children,
        }
    }
}

impl Component for TranslateGroup {
    fn view(&self) -> View {
        View::Translate {
            tx: (self.tx)(),
            ty: (self.ty)(),
            children: self.children.iter().map(|c| c.view()).collect(),
        }
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        let tx = (self.tx)() as f64;
        let ty = (self.ty)() as f64;
        let translated = offset_pointer(event, -tx, -ty);
        let effective = translated.as_ref().unwrap_or(event);
        dispatch_to_children(&mut self.children, effective)
    }
}
