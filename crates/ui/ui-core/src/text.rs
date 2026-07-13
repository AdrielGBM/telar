use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use geometry_core::Rect;
use layout_core::{LayoutError, LayoutStyle};
use platform_core::Event;
use renderer_core::TextStyle;
use ui_tree::{Component, EventResult, RenderNode};

use crate::impl_leaf_widget;
use crate::layout_leaf::LayoutLeaf;

pub struct Text {
    content: Rc<dyn Fn() -> String>,
    cached_content: RefCell<(String, Arc<str>)>,
    // Ink bounds memo for optical vertical centering: (text, width_bits) -> (ink_top, ink_height).
    // Recomputed only when the text or its resolved width changes, so a static label costs no per-frame shaping.
    cached_ink: RefCell<Option<(String, u32, f32, f32)>>,
    style: Rc<dyn Fn() -> TextStyle>,
    leaf: LayoutLeaf,
}

impl Text {
    pub fn new(
        content_fn: impl Fn() -> String + 'static,
        layout_style: LayoutStyle,
        style_fn: impl Fn() -> TextStyle + 'static,
    ) -> Result<Self, LayoutError> {
        // Stretch overrides any parent align-items (e.g. center) so text always fills the parent's cross-axis width instead of collapsing to 0.
        let leaf = LayoutLeaf::register(layout_style.align_self_stretch())?;
        Ok(Self {
            content: Rc::new(content_fn),
            cached_content: RefCell::new((String::new(), Arc::from(""))),
            cached_ink: RefCell::new(None),
            style: Rc::new(style_fn),
            leaf,
        })
    }

    /// Like [`Text::new`], but the leaf's height is measured from the content at its
    /// resolved width, so the box grows to fit however many lines the text wraps
    /// into and pushes following siblings down instead of overflowing onto them.
    pub fn auto(
        content_fn: impl Fn() -> String + 'static,
        layout_style: LayoutStyle,
        style_fn: impl Fn() -> TextStyle + 'static,
    ) -> Result<Self, LayoutError> {
        let content_fn: Rc<dyn Fn() -> String> = Rc::new(content_fn);
        let style: Rc<dyn Fn() -> TextStyle> = Rc::new(style_fn);

        let measure_content = Rc::clone(&content_fn);
        let measure_style = Rc::clone(&style);
        let measure = Box::new(move |max_width: f32| {
            let s = (measure_style)();
            renderer_text::measure_text(&(measure_content)(), max_width, &s)
        });

        let (node, rect) =
            crate::context::new_measured_leaf(layout_style.align_self_stretch(), measure)?;
        Ok(Self {
            content: content_fn,
            cached_content: RefCell::new((String::new(), Arc::from(""))),
            cached_ink: RefCell::new(None),
            style,
            leaf: LayoutLeaf { node, rect },
        })
    }

    pub fn single_line(
        content_fn: impl Fn() -> String + 'static,
        style_fn: impl Fn() -> TextStyle + 'static,
    ) -> Result<Self, LayoutError> {
        let height = style_fn().font_size * 1.4;
        Text::new(content_fn, LayoutStyle::new().height(height), style_fn)
    }
}

impl Component for Text {
    fn view(&self) -> RenderNode {
        let r = self.leaf.rect.get();
        let text: Arc<str> = {
            let new_str = (self.content)();
            let mut cache = self.cached_content.borrow_mut();
            if cache.0 != new_str {
                let rc = Arc::from(new_str.as_str());
                *cache = (new_str, Arc::clone(&rc));
                rc
            } else {
                Arc::clone(&cache.1)
            }
        };
        let style = (self.style)();
        // Optically center the text's INK within the leaf. A text leaf stretches to fill its parent's cross
        // axis (`align_self_stretch`), and the font's line box reserves ascent room for accents/descenders
        // that a short run ("72%") leaves empty — so line-box-centered text sits visibly high next to an
        // icon. Centering the actual drawn glyph extent lines the two up. Memoized per (text, width).
        let (ink_top, ink_height) = {
            let width_bits = r.width.to_bits();
            let mut cache = self.cached_ink.borrow_mut();
            match cache.as_ref() {
                Some((t, w, top, h)) if *t == *text && *w == width_bits => (*top, *h),
                _ => {
                    let (top, h) = renderer_text::measure_ink_bounds(&text, r.width, &style);
                    *cache = Some((text.to_string(), width_bits, top, h));
                    (top, h)
                }
            }
        };
        // Render the full line box (so nothing clips), offset so the ink's own center lands on the leaf's
        // center. When there is no ink (empty run) fall back to a top-aligned box.
        let (_, line_height) = renderer_text::measure_text(&text, r.width, &style);
        let y = if ink_height > 0.0 {
            r.height / 2.0 - ink_top - ink_height / 2.0
        } else {
            0.0
        };
        self.leaf.at_layout_position(RenderNode::text(
            text,
            Rect {
                x: 0.0,
                y,
                width: r.width,
                height: line_height,
            },
            style,
        ))
    }

    fn on_event(&mut self, _event: &Event) -> EventResult {
        EventResult::Ignored
    }

    fn debug_name(&self) -> &'static str {
        "Text"
    }
}

impl_leaf_widget!(Text);
