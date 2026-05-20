use platform_core::Event;
use ui_tree::{Component, EventResult, View};

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

    pub fn static_offset(tx: f32, ty: f32, children: Vec<Box<dyn Component>>) -> Self {
        Self {
            tx: Box::new(move || tx),
            ty: Box::new(move || ty),
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
        for child in &mut self.children {
            if child.on_event(event) == EventResult::Handled {
                return EventResult::Handled;
            }
        }
        EventResult::Ignored
    }
}
