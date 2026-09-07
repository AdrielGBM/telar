//! [`Canvas`]: a leaf that hands its rect to a closure and lets it draw whatever it likes.

use crate::impl_leaf_widget;
use crate::layout_leaf::LayoutLeaf;
use geometry_core::Rect;
use layout_core::{LayoutError, LayoutStyle};
use platform_core::Event;
use renderer_core::TextStyle;
use ui_tree::{Component, EventResult, RenderNode};

/// Where a canvas gets the paint for its artwork: from itself, or from the text the tree above it declared.
enum Artwork {
    Fixed(Box<dyn Fn(Rect) -> RenderNode>),
    Inheriting(Box<dyn Fn(Rect, TextStyle) -> RenderNode>),
}

/// A leaf that hands its laid-out rect to a closure and lets it draw whatever it likes.
pub struct Canvas {
    leaf: LayoutLeaf,
    draw: Artwork,
}

impl Canvas {
    pub fn new(
        layout_style: LayoutStyle,
        draw_fn: impl Fn(Rect) -> RenderNode + 'static,
    ) -> Result<Self, LayoutError> {
        let leaf = LayoutLeaf::register(layout_style)?;
        Ok(Self {
            leaf,
            draw: Artwork::Fixed(Box::new(draw_fn)),
        })
    }

    /// A canvas whose artwork is handed the text style the tree above it declared.
    ///
    /// For the glyphs an interface draws rather than spells — a caret, a tick, a chevron — because a font cannot be relied on to carry them at the size and weight the label beside them is set in. Drawn, they stop being text and lose the cascade with it: the caret on a `select` stayed the theme's ink in a region that had declared its own, while the label it points at followed. This is CSS's `currentColor`, for the shapes a face does not supply.
    pub fn declaring(
        layout_style: LayoutStyle,
        draw_fn: impl Fn(Rect, TextStyle) -> RenderNode + 'static,
    ) -> Result<Self, LayoutError> {
        let leaf = LayoutLeaf::register(layout_style)?;
        Ok(Self {
            leaf,
            draw: Artwork::Inheriting(Box::new(draw_fn)),
        })
    }
}

impl Canvas {
    pub fn with_intrinsic_height(
        height: f32,
        draw_fn: impl Fn(geometry_core::Rect) -> ui_tree::RenderNode + 'static,
    ) -> Result<Self, layout_core::LayoutError> {
        Self::new(layout_core::LayoutStyle::new().height(height), draw_fn)
    }
}

impl_leaf_widget!(Canvas);

impl Component for Canvas {
    fn view(&self) -> RenderNode {
        let r = self.leaf.rect.get();
        // A Canvas closure draws at fixed coordinates that ignore the layout rect, so a collapsed rect would still paint over other content.
        if r.width <= 0.0 || r.height <= 0.0 {
            return RenderNode::Empty;
        }
        // `at_layout_position` translates the output, so a zero-origin rect avoids double-offsetting anything derived from `rect.x`/`rect.y`.
        let local = Rect {
            x: 0.0,
            y: 0.0,
            width: r.width,
            height: r.height,
        };
        let inner = match &self.draw {
            Artwork::Fixed(draw) => draw(local),
            Artwork::Inheriting(draw) => {
                draw(local, crate::inherit::inherited_text_style(self.leaf.node))
            }
        };
        self.leaf
            .at_layout_position_as(renderer_core::Semantics::drawing, inner)
    }

    fn on_event(&mut self, _event: &Event) -> EventResult {
        EventResult::Ignored
    }

    fn debug_name(&self) -> &'static str {
        "Canvas"
    }
}

#[cfg(test)]
#[path = "canvas_test.rs"]
mod tests;
