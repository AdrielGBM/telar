use platform_core::Event;
use renderer_core::RendererError;
use ui_core::ComponentTree;

use crate::app::{App, Frame};
use crate::app_context::AppCtx;
use crate::reactive_app::ReactiveApp;

pub(crate) struct ReactiveAdapter {
    app: Box<dyn ReactiveApp>,
    tree: Option<ComponentTree>,
}

impl ReactiveAdapter {
    pub fn new(app: impl ReactiveApp) -> Self {
        Self {
            app: Box::new(app),
            tree: None,
        }
    }
}

impl App for ReactiveAdapter {
    fn on_resume(&mut self, _ctx: &mut AppCtx) -> Result<(), RendererError> {
        self.tree = Some(ComponentTree::new(self.app.root()));
        Ok(())
    }

    fn on_event(&mut self, event: Event, _ctx: &mut AppCtx) {
        if let Some(tree) = &mut self.tree {
            tree.on_event(&event);
        }
    }

    fn on_redraw(&mut self, frame: &mut Frame, _ctx: &mut AppCtx) {
        if let Some(color) = self.app.clear_color() {
            frame.clear(color);
        }
        if let Some(tree) = &self.tree {
            frame.extend(tree.commands());
        }
    }

    fn on_suspend(&mut self, _ctx: &mut AppCtx) {}
}
