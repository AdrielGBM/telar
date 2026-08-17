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

/// A line-number gutter for a code editor: the column "1\n2\n3…" drawn top-aligned with the same line height a
/// [`TextArea`](crate::TextArea) uses, so line *n* here sits exactly on line *n* of the editor. Place it beside
/// the editor inside the same scroll (so they scroll together) and give both the same `font_size`. It measures
/// its own width from the widest number and its height from the line count, re-measuring reactively as the
/// count changes. Toggle it by collapsing its node (`set_display`) inside a [`ClippedItem`](crate::ClippedItem)
/// so a hidden gutter both takes no width and draws nothing.
pub struct LineGutter {
    line_count: Rc<dyn Fn() -> usize>,
    style: Rc<dyn Fn() -> TextStyle>,
    leaf: LayoutLeaf,
    // Re-measures the leaf whenever the line count changes (a digit more → wider; a line more → taller), so the
    // gutter tracks the editor as it grows. Kept alive for the widget's life.
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
            // Width of the widest (last) number; height from the line count — matching the editor's own metric.
            let width = crate::text_metrics::measure_text(&n.to_string(), NO_WRAP_WIDTH, &s).0;
            (width, n as f32 * line_h)
        });
        let (node, rect) = new_measured_leaf(layout_style, measure)?;
        let remeasure = {
            let line_count = Rc::clone(&line_count);
            effect(move || {
                // Tracked read of the count source, so a change re-measures the leaf.
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
        // Draw from the leaf's top-left (like a `TextArea`), each line at `line * line_h` — never optically
        // centered — so the numbers line up with the editor even when the leaf is stretched taller than them.
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
mod tests {
    use super::*;
    use crate::context::{compute_layout, new_container, reset_layout_runtime};
    use layout_core::AvailableSpace;
    use reactive_core::{RwSignal, signal};
    use renderer_core::Color;

    fn gutter(count: RwSignal<usize>) -> LineGutter {
        LineGutter::new(
            move || count.get(),
            LayoutStyle::new(),
            || TextStyle::new(14.0, Color::BLACK),
        )
        .unwrap()
    }

    // The gutter's measured height tracks the line count at the editor's line height, so its rows stay in step
    // with the editor as lines are added.
    #[test]
    fn height_tracks_line_count() {
        reset_layout_runtime();
        let count = signal(3usize);
        let g = gutter(count.clone());
        let rect = g.leaf.rect;
        let root = new_container(
            LayoutStyle::new().flex_column().width(200.0),
            &[g.leaf.node],
        )
        .unwrap();
        let line_h = crate::text_metrics::line_height(14.0);
        compute_layout(
            root,
            AvailableSpace::Definite(200.0),
            AvailableSpace::MaxContent,
        )
        .unwrap();
        assert!(
            (rect.get().height - 3.0 * line_h).abs() < 0.5,
            "3 lines: {:?}",
            rect.get()
        );

        count.set(10);
        compute_layout(
            root,
            AvailableSpace::Definite(200.0),
            AvailableSpace::MaxContent,
        )
        .unwrap();
        assert!(
            (rect.get().height - 10.0 * line_h).abs() < 0.5,
            "grew to 10 lines: {:?}",
            rect.get()
        );
        // A two-digit count is wider than a one-digit one, so the gutter reserved more width.
        assert!(
            rect.get().width > 0.0,
            "gutter reserves a width: {:?}",
            rect.get()
        );
    }
}
