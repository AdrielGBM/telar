use std::rc::Rc;
use std::sync::Arc;

use geometry_core::Rect;
use layout_core::{LayoutError, LayoutStyle};
use platform_core::Event;
use renderer_core::{TextRun, TextStyle};
use ui_tree::{Component, EventResult, RenderNode};

use crate::impl_leaf_widget;
use crate::layout_leaf::LayoutLeaf;

/// A paragraph of mixed-style text: a sequence of [`TextRun`]s (bold, italic, coloured links) shaped and
/// wrapped as one, the multi-style counterpart of [`Text`](crate::Text). The shared paragraph metrics — font
/// size, line height, wrapping, `max_lines` — come from a base [`TextStyle`]; each run overrides only weight,
/// slant, and colour.
pub struct RichText {
    runs: Rc<dyn Fn() -> Vec<TextRun>>,
    base: Rc<dyn Fn() -> TextStyle>,
    leaf: LayoutLeaf,
}

impl RichText {
    /// A rich paragraph whose leaf height is measured from its runs at the resolved width, so the box grows to
    /// fit however many lines they wrap into and pushes following siblings down — like [`Text::auto`].
    pub fn auto(
        runs_fn: impl Fn() -> Vec<TextRun> + 'static,
        layout_style: LayoutStyle,
        base_fn: impl Fn() -> TextStyle + 'static,
    ) -> Result<Self, LayoutError> {
        let runs: Rc<dyn Fn() -> Vec<TextRun>> = Rc::new(runs_fn);
        let base: Rc<dyn Fn() -> TextStyle> = Rc::new(base_fn);

        let measure_runs = Rc::clone(&runs);
        let measure_base = Rc::clone(&base);
        let measure = Box::new(move |max_width: f32| {
            crate::text_metrics::measure_rich_text(&(measure_runs)(), max_width, &(measure_base)())
        });

        let (node, rect) =
            crate::context::new_measured_leaf(layout_style.align_self_stretch(), measure)?;
        Ok(Self {
            runs,
            base,
            leaf: LayoutLeaf { node, rect },
        })
    }
}

impl Component for RichText {
    fn view(&self) -> RenderNode {
        let r = self.leaf.rect.get();
        let runs: Arc<[TextRun]> = Arc::from((self.runs)());
        let base = (self.base)();
        self.leaf.at_layout_position(RenderNode::rich_text(
            runs,
            Rect {
                x: 0.0,
                y: 0.0,
                width: r.width,
                height: r.height,
            },
            base,
        ))
    }

    fn on_event(&mut self, _event: &Event) -> EventResult {
        EventResult::Ignored
    }

    fn debug_name(&self) -> &'static str {
        "RichText"
    }
}

impl_leaf_widget!(RichText);
