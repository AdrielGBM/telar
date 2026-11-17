//! [`Path`]: a vector-geometry leaf that lays itself out and paints its own fill and stroke.

use std::sync::Arc;

use geometry_core::Rect;
use layout_core::{LayoutError, LayoutStyle};
use platform_core::Event;
use renderer_core::{PathData, PathStyle, TextStyle};
use ui_tree::{Component, EventResult, RenderNode};

use crate::impl_leaf_widget;
use crate::layout_leaf::LayoutLeaf;

/// The shape a path draws and the paint it draws it in.
type Artwork = (Arc<PathData>, PathStyle);

type DrawnInText = Box<dyn Fn(Rect, &TextStyle) -> Artwork>;

/// Where a path gets its shape and paint: from its box alone, or from its box and the text the tree above it declared.
enum Draw {
    Fixed(Box<dyn Fn(Rect) -> Artwork>),
    Inheriting(DrawnInText),
}

/// Vector artwork as a widget: a shape a `col` can hold, size and place like anything else.
///
/// The path's own coordinates are local to the box it is given, and the box's position is applied over them — so a shape drawn from `0,0` lands where layout put it. Its closure is handed that box at the origin, so artwork drawn in other units can be scaled into it, and runs on every paint, so a signal it reads repaints the path.
pub struct Path {
    leaf: LayoutLeaf,
    draw: Draw,
}

impl Path {
    pub fn new(
        layout_style: LayoutStyle,
        draw: impl Fn(Rect) -> (Arc<PathData>, PathStyle) + 'static,
    ) -> Result<Self, LayoutError> {
        Ok(Self {
            leaf: LayoutLeaf::register(layout_style)?,
            draw: Draw::Fixed(Box::new(draw)),
        })
    }

    /// A path whose shape or paint reads the text style the tree above it declared: a drawn mark that follows the ink of the region it sits in, as the text beside it does, and repaints when that declaration changes.
    pub fn declaring(
        layout_style: LayoutStyle,
        draw: impl Fn(Rect, &TextStyle) -> (Arc<PathData>, PathStyle) + 'static,
    ) -> Result<Self, LayoutError> {
        Ok(Self {
            leaf: LayoutLeaf::register(layout_style)?,
            draw: Draw::Inheriting(Box::new(draw)),
        })
    }
}

impl_leaf_widget!(Path);

impl Component for Path {
    fn view(&self) -> RenderNode {
        let rect = self.leaf.rect.get();
        // The geometry is in its own coordinates, so a collapsed box would paint over whatever made room for it.
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return RenderNode::Empty;
        }
        let local = Rect::new(0.0, 0.0, rect.width, rect.height);
        let (data, style) = match &self.draw {
            Draw::Fixed(draw) => draw(local),
            Draw::Inheriting(draw) => {
                draw(local, &crate::inherit::inherited_text_style(self.leaf.node))
            }
        };
        self.leaf.at_layout_position_as(
            renderer_core::Semantics::drawing,
            RenderNode::path(data, style),
        )
    }

    fn on_event(&mut self, _event: &Event) -> EventResult {
        EventResult::Ignored
    }

    fn debug_name(&self) -> &'static str {
        "Path"
    }
}

#[cfg(test)]
#[path = "path.test.rs"]
mod tests;
