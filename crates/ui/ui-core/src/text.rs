use std::cell::RefCell;
use std::sync::Arc;

use geometry_core::Rect;
use layout_core::{LayoutError, LayoutStyle};
use platform_core::Event;
use renderer_core::TextStyle;
use ui_tree::{Component, EventResult, RenderNode};

use crate::impl_leaf_widget;
use crate::layout_leaf::LayoutLeaf;

pub struct Text {
    content_fn: Box<dyn Fn() -> String>,
    cached_content: RefCell<(String, Arc<str>)>,
    style: Box<dyn Fn() -> TextStyle>,
    leaf: LayoutLeaf,
}

impl Text {
    pub fn new(
        ctx: &mut crate::context::WidgetCtx,
        content_fn: impl Fn() -> String + 'static,
        layout_style: LayoutStyle,
        style_fn: impl Fn() -> TextStyle + 'static,
    ) -> Result<Self, LayoutError> {
        // Stretch overrides any parent align-items (e.g. center) so text always
        // fills the parent's cross-axis width instead of collapsing to 0.
        let leaf = LayoutLeaf::register(ctx, layout_style.align_self_stretch())?;
        Ok(Self {
            content_fn: Box::new(content_fn),
            cached_content: RefCell::new((String::new(), Arc::from(""))),
            style: Box::new(style_fn),
            leaf,
        })
    }

    pub fn single_line(
        ctx: &mut crate::context::WidgetCtx,
        content_fn: impl Fn() -> String + 'static,
        style_fn: impl Fn() -> TextStyle + 'static,
    ) -> Result<Self, LayoutError> {
        let height = style_fn().font_size * 1.4;
        Text::new(ctx, content_fn, LayoutStyle::new().height(height), style_fn)
    }
}

impl Component for Text {
    fn view(&self) -> RenderNode {
        let r = self.leaf.rect.get();
        let text: Arc<str> = {
            let new_str = (self.content_fn)();
            let mut cache = self.cached_content.borrow_mut();
            if cache.0 != new_str {
                let rc = Arc::from(new_str.as_str());
                *cache = (new_str, Arc::clone(&rc));
                rc
            } else {
                Arc::clone(&cache.1)
            }
        };
        self.leaf.at_layout_position(RenderNode::text(
            text,
            Rect {
                x: 0.0,
                y: 0.0,
                width: r.width,
                height: r.height,
            },
            (self.style)(),
        ))
    }

    fn on_event(&mut self, _event: &Event) -> EventResult {
        EventResult::Ignored
    }
}

impl_leaf_widget!(Text);
