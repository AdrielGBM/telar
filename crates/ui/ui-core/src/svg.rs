//! [`Svg`]: a vector-artwork leaf, sized by `object-fit` and optionally tinted.

use std::sync::Arc;

use geometry_core::{ObjectFit, Rect};
use layout_core::{LayoutError, LayoutStyle};
use platform_core::Event;
use renderer_assets::SvgData;
use renderer_core::{BorderRadius, Color};
use ui_tree::{Component, EventResult, NodeVec, RenderNode};

use crate::impl_leaf_widget;
use crate::layout_leaf::LayoutLeaf;

/// A vector-artwork leaf, sized by `object-fit` and optionally tinted.
pub struct Svg {
    data: Box<dyn Fn() -> Arc<SvgData>>,
    tint: Box<dyn Fn() -> Option<Color>>,
    stroke: Box<dyn Fn() -> Option<f32>>,
    fit: Box<dyn Fn() -> ObjectFit>,
    leaf: LayoutLeaf,
}

impl Svg {
    pub fn new(
        layout_style: LayoutStyle,
        data_fn: impl Fn() -> Arc<SvgData> + 'static,
        tint_fn: impl Fn() -> Option<Color> + 'static,
        fit_fn: impl Fn() -> ObjectFit + 'static,
    ) -> Result<Self, LayoutError> {
        // A side left at `auto` falls back to the intrinsic size, a single px side derives the other from the intrinsic aspect ratio, and a percent side is left untouched.
        let layout_style =
            crate::layout_leaf::resolve_intrinsic_size(layout_style, || data_fn().intrinsic_size());

        let leaf = LayoutLeaf::register(layout_style)?;
        Ok(Self {
            data: Box::new(data_fn),
            tint: Box::new(tint_fn),
            stroke: Box::new(|| None),
            fit: Box::new(fit_fn),
            leaf,
        })
    }

    /// Overrides every stroked path's width (SVG userspace units, e.g. Lucide's `2`) — the theme's icon-stroke token. Reactive like `tint`: re-read on every `view()`, so a live theme change restrokes the glyph. `None` keeps the SVG's own widths.
    pub fn with_stroke(mut self, stroke_fn: impl Fn() -> Option<f32> + 'static) -> Self {
        self.stroke = Box::new(stroke_fn);
        self
    }
}

impl Component for Svg {
    fn view(&self) -> RenderNode {
        let r = self.leaf.rect.get();
        let fit = (self.fit)();
        let commands =
            (self.data)().commands_for(r.width, r.height, (self.tint)(), (self.stroke)(), fit);
        let children = NodeVec::collect(commands.iter().cloned().map(RenderNode::Primitive));
        let group = RenderNode::Group { children };
        // Cover scales the paths past the box. The renderer maps clip rects through the active matrix, so a local (0,0,w,h) clip composes with this widget's transform and any scroll.
        let node = if fit == ObjectFit::Cover {
            RenderNode::clip(
                Rect::new(0.0, 0.0, r.width, r.height),
                BorderRadius::zero(),
                [group],
            )
        } else {
            group
        };
        self.leaf
            .at_layout_position_as(renderer_core::Semantics::drawing, node)
    }

    fn on_event(&mut self, _event: &Event) -> EventResult {
        EventResult::Ignored
    }

    fn debug_name(&self) -> &'static str {
        "Svg"
    }
}

impl_leaf_widget!(Svg);

// Gated on `dynamic-svg`: these build `SvgData` with `from_str`, unavailable under bare `svg`.
#[cfg(all(test, feature = "dynamic-svg"))]
#[path = "svg_test.rs"]
mod tests;
