//! [`LineGutter`]: the line-number column beside a text area, measured to track its line count.

use std::rc::Rc;

use geometry_core::Rect;
use layout_core::{LayoutError, LayoutStyle};
use platform_core::Event;
use reactive_core::{Effect, effect};
use renderer_core::TextStyle;
use ui_tree::{Component, EventResult, RenderNode};

use crate::context::{mark_dirty, new_measured_leaf};
use crate::impl_leaf_widget;
use crate::layout_leaf::LayoutLeaf;

/// A width so large the shaper never soft-wraps a line number.
const NO_WRAP_WIDTH: f32 = 1.0e6;

/// A line-number gutter for a code editor: the column "1\n2\n3…" drawn top-aligned with the same line height a [`TextArea`](crate::TextArea) uses, so line *n* here sits exactly on line *n* of the editor. Place it beside the editor inside the same scroll (so they scroll together) and give both the same `font_size`. It measures its own width from the widest number and its height from the line count, re-measuring reactively as the count changes. Toggle it by collapsing its node (`set_display`) inside a [`ClippedItem`](crate::ClippedItem) so a hidden gutter both takes no width and draws nothing.
pub struct LineGutter {
    line_count: Rc<dyn Fn() -> usize>,
    style: Rc<dyn Fn() -> TextStyle>,
    leaf: LayoutLeaf,
    // Re-measures whenever the line count changes, so the gutter tracks the editor as it grows.
    _remeasure: Effect,
}

impl LineGutter {
    pub fn new(
        line_count: impl Fn() -> usize + 'static,
        layout_style: LayoutStyle,
        style_fn: impl Fn() -> TextStyle + 'static,
    ) -> Result<Self, LayoutError> {
        let line_count: Rc<dyn Fn() -> usize> = Rc::new(line_count);
        let style: Rc<dyn Fn() -> TextStyle> = Rc::new(style_fn);

        let measure_count = Rc::clone(&line_count);
        let measure_style = Rc::clone(&style);
        let measure = Box::new(move |_max_width: f32| {
            let s = (measure_style)();
            let line_h = crate::text_metrics::line_height(s.font_size);
            let n = (measure_count)().max(1);
            // Width of the widest (last) number; height from the line count, matching the editor's own metric.
            let width =
                crate::text_metrics::measure_text(&n.to_string(), None, NO_WRAP_WIDTH, &s).0;
            (width, n as f32 * line_h)
        });
        let (node, rect) = new_measured_leaf(layout_style, measure)?;
        let remeasure = {
            let line_count = Rc::clone(&line_count);
            effect(move || {
                // A tracked read of the count source, so a change re-measures the leaf.
                let _ = (line_count)();
                mark_dirty(node).ok();
            })
        };
        Ok(Self {
            line_count,
            style,
            leaf: LayoutLeaf { node, rect },
            _remeasure: remeasure,
        })
    }
}

impl Component for LineGutter {
    fn view(&self) -> RenderNode {
        let style = (self.style)();
        let line_h = crate::text_metrics::line_height(style.font_size);
        let n = (self.line_count)().max(1);
        let mut numbers = String::new();
        for i in 1..=n {
            if i > 1 {
                numbers.push('\n');
            }
            numbers.push_str(&i.to_string());
        }
        let r = self.leaf.rect.get();
        // From the leaf's top-left, never optically centred, so the numbers line up with the editor even when the leaf is stretched taller than them.
        let full = Rect {
            x: 0.0,
            y: 0.0,
            width: r.width.max(1.0),
            height: (n as f32 * line_h).max(line_h),
        };
        self.leaf
            .at_layout_position(RenderNode::text(numbers, full, style))
    }

    fn on_event(&mut self, _event: &Event) -> EventResult {
        EventResult::Ignored
    }

    fn debug_name(&self) -> &'static str {
        "LineGutter"
    }
}

impl_leaf_widget!(LineGutter);

#[cfg(test)]
#[path = "line_gutter_test.rs"]
mod tests;
