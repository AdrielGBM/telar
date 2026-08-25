use std::sync::Arc;

use layout_core::{LayoutError, LayoutStyle};
use platform_core::Event;
use renderer_core::{PathData, PathStyle};
use ui_tree::{Component, EventResult, RenderNode};

use crate::impl_leaf_widget;
use crate::layout_leaf::LayoutLeaf;

/// Vector artwork as a widget: a shape a `col` can hold, size and place like anything else.
///
/// The path's own coordinates are local to the box it is given, and the box's position is applied over them
/// — so a shape drawn from `0,0` lands where layout put it. It had to be wrapped in a [`Canvas`](crate::Canvas)
/// to reach a layout at all, which is a wrapper the caller had to know to write and the transpiler emitted
/// around every `path` in markup.
pub struct Path {
    leaf: LayoutLeaf,
    data: Box<dyn Fn() -> Arc<PathData>>,
    style: Box<dyn Fn() -> PathStyle>,
}

impl Path {
    pub fn new(
        layout_style: LayoutStyle,
        data: impl Fn() -> Arc<PathData> + 'static,
        style: impl Fn() -> PathStyle + 'static,
    ) -> Result<Self, LayoutError> {
        Ok(Self {
            leaf: LayoutLeaf::register(layout_style)?,
            data: Box::new(data),
            style: Box::new(style),
        })
    }

    /// A path whose geometry never changes — artwork baked at build time, where only the paint is reactive.
    pub fn static_data(
        layout_style: LayoutStyle,
        data: Arc<PathData>,
        style: impl Fn() -> PathStyle + 'static,
    ) -> Result<Self, LayoutError> {
        Self::new(layout_style, move || data.clone(), style)
    }
}

impl_leaf_widget!(Path);

impl Component for Path {
    fn view(&self) -> RenderNode {
        let rect = self.leaf.rect.get();
        // A collapsed box draws nothing: the geometry is in its own coordinates and would otherwise paint
        // over whatever a `display:none` was meant to make room for.
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return RenderNode::Empty;
        }
        self.leaf
            .at_layout_position(RenderNode::path((self.data)(), (self.style)()))
    }

    fn on_event(&mut self, _event: &Event) -> EventResult {
        EventResult::Ignored
    }

    fn debug_name(&self) -> &'static str {
        "Path"
    }
}
