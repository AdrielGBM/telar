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
        for child in &mut self.children {
            if child.on_event(effective).is_handled() {
                return EventResult::Handled;
            }
        }
        EventResult::Ignored
    }
}

fn offset_pointer(event: &Event, dx: f64, dy: f64) -> Option<Event> {
    match event {
        Event::PointerMoved { x, y, source } => Some(Event::PointerMoved {
            x: x + dx,
            y: y + dy,
            source: source.clone(),
        }),
        Event::PointerPressed {
            x,
            y,
            button,
            source,
        } => Some(Event::PointerPressed {
            x: x + dx,
            y: y + dy,
            button: button.clone(),
            source: source.clone(),
        }),
        Event::PointerReleased {
            x,
            y,
            button,
            source,
        } => Some(Event::PointerReleased {
            x: x + dx,
            y: y + dy,
            button: button.clone(),
            source: source.clone(),
        }),
        _ => None,
    }
}
