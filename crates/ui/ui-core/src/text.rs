//! [`Text`]: the measured text leaf, optically centred in the box its parent gave it.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use geometry_core::Rect;
use layout_core::{LayoutError, LayoutStyle};
use platform_core::Event;
use reactive_core::{Effect, effect};
use renderer_core::{Span, TextStyle};
use ui_tree::{Component, EventResult, RenderNode};

use crate::context::mark_dirty;
use crate::impl_leaf_widget;
use crate::layout_leaf::LayoutLeaf;

/// The run the glyph band is measured from: a capital, an x-height letter and a descender, which between them span the extent a Latin face actually draws in. Any string of the same style is then centred by the same amount, which is what puts a row of labels on one baseline.
const REFERENCE: &str = "Hxg";
/// Room the reference cannot fill: it is three characters, and cosmic-text overflows on an unbounded one.
const REFERENCE_WIDTH: f32 = 1_000.0;

/// The measured text leaf, optically centred in the box its parent gave it.
pub struct Text {
    content: Rc<dyn Fn() -> String>,
    // The byte ranges that restyle themselves, or `None` for a paragraph without any.
    spans: Option<Rc<dyn Fn() -> Vec<Span>>>,
    cached_content: RefCell<(String, Arc<str>)>,
    // font_size bits -> (ink_top, ink_height, line_height). Keyed on size and not on text, because the band is measured from a reference run.
    cached_ink: RefCell<Option<(u32, f32, f32, f32)>>,
    style: Rc<dyn Fn() -> TextStyle>,
    leaf: LayoutLeaf,
    // Held for its subscription: without it a measured leaf keeps the width the previous string wanted, and a label that grew soft-wraps into a slot built for the old text.
    _remeasure: Option<Effect>,
}

/// Where a text gets its style: given whole, or derived from what the tree above it declared.
enum StyleSource {
    Complete(Rc<dyn Fn() -> TextStyle>),
    Inheriting(Rc<dyn Fn(TextStyle) -> TextStyle>),
}

impl Text {
    /// A text leaf whose height is measured from its content at the resolved width, so the box grows to fit however many lines the text wraps into and pushes following siblings down instead of overflowing.
    ///
    /// There is no fixed-height counterpart: an explicit `height` in `layout_style` pins the box, which is what the second constructor was for. Choosing between them by whether the markup happened to write a height meant the same label measured or did not depending on a detail of how it was asked for.
    pub fn new(
        content_fn: impl Fn() -> String + 'static,
        layout_style: LayoutStyle,
        style_fn: impl Fn() -> TextStyle + 'static,
    ) -> Result<Self, LayoutError> {
        Self::build(
            Rc::new(content_fn),
            None,
            layout_style,
            StyleSource::Complete(Rc::new(style_fn)),
        )
    }

    /// A text styled by what the tree above it declared, amended by whatever it says for itself.
    ///
    /// The amendment takes the inherited style and returns the final one, rather than naming properties, because a leaf has two kinds of thing to say: an override of something inherited (`font_size`) and a clamp that could never be inherited (`max_lines`). One closure carries both, and it is the shape a caller already amends a `RectStyle` with.
    ///
    /// [`new`](Self::new) is the opt-out: passing a whole `TextStyle` is the honest way to say the tree above has no business in this one.
    pub fn declaring(
        content_fn: impl Fn() -> String + 'static,
        layout_style: LayoutStyle,
        style_fn: impl Fn(TextStyle) -> TextStyle + 'static,
    ) -> Result<Self, LayoutError> {
        Self::build(
            Rc::new(content_fn),
            None,
            layout_style,
            StyleSource::Inheriting(Rc::new(style_fn)),
        )
    }

    /// [`new`](Self::new) with byte ranges that style themselves differently from the paragraph — a bold word, a coloured link — shaped and wrapped as one text rather than as separate widgets.
    pub fn spanned(
        content_fn: impl Fn() -> String + 'static,
        spans_fn: impl Fn() -> Vec<Span> + 'static,
        layout_style: LayoutStyle,
        style_fn: impl Fn() -> TextStyle + 'static,
    ) -> Result<Self, LayoutError> {
        Self::build(
            Rc::new(content_fn),
            Some(Rc::new(spans_fn)),
            layout_style,
            StyleSource::Complete(Rc::new(style_fn)),
        )
    }

    fn build(
        content_fn: Rc<dyn Fn() -> String>,
        spans_fn: Option<Rc<dyn Fn() -> Vec<Span>>>,
        layout_style: LayoutStyle,
        source: StyleSource,
    ) -> Result<Self, LayoutError> {
        // The node does not exist until the leaf is registered, and that call's measure closure already reads the style, so the cell lets the style close over a node older than itself.
        let node_cell = Rc::new(std::cell::Cell::new(None::<layout_core::NodeId>));
        let style: Rc<dyn Fn() -> TextStyle> = match source {
            StyleSource::Complete(style_fn) => style_fn,
            StyleSource::Inheriting(amend) => {
                let cell = Rc::clone(&node_cell);
                Rc::new(move || {
                    let inherited = match cell.get() {
                        Some(node) => crate::inherit::inherited_text_style(node),
                        None => crate::inherit::Inherited::initial().text_style(),
                    };
                    amend(inherited)
                })
            }
        };

        let measure_content = Rc::clone(&content_fn);
        let measure_style = Rc::clone(&style);
        let measure_spans = spans_fn.clone();
        let measure = Box::new(move |max_width: f32| {
            let s = (measure_style)();
            // Spans change the extent, so a box measured without them wraps differently from the text drawn into it.
            let spans = measure_spans.as_ref().map(|f| f());
            crate::text_metrics::measure_text(&(measure_content)(), spans.as_deref(), max_width, &s)
        });

        // Stretch overrides any parent align-items, so text fills the cross axis instead of collapsing to 0.
        let (node, rect) =
            crate::context::new_measured_leaf(layout_style.align_self_stretch(), measure)?;
        node_cell.set(Some(node));
        // Read through the measure closure, so it subscribes to exactly what the measure depends on: a signal re-set to its own value would otherwise cost a shaping pass and a relayout for nothing.
        let dirty_content = Rc::clone(&content_fn);
        let measured = RefCell::new(Option::<String>::None);
        let remeasure = effect(move || {
            let next = (dirty_content)();
            if measured.borrow().as_deref() == Some(next.as_str()) {
                return;
            }
            *measured.borrow_mut() = Some(next);
            mark_dirty(node).ok();
        });
        Ok(Self {
            content: content_fn,
            spans: spans_fn,
            cached_content: RefCell::new((String::new(), Arc::from(""))),
            cached_ink: RefCell::new(None),
            style,
            leaf: LayoutLeaf { node, rect },
            _remeasure: Some(remeasure),
        })
    }

    pub fn single_line(
        content_fn: impl Fn() -> String + 'static,
        style_fn: impl Fn() -> TextStyle + 'static,
    ) -> Result<Self, LayoutError> {
        // Asked of the installed measurer rather than multiplied out here: a terminal draws one glyph per cell whatever size the text claims, so its answer is a cell — and a title at 32px reserving 44.8px of box for a single row of glyphs is how a big letter used to push everything below it out of the grid.
        let height = crate::text_metrics::single_line_box(style_fn().font_size);
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
        // A text leaf stretches to fill its parent's cross axis, and the font's line box reserves ascent room a run never uses, so line-box-centred text sits visibly high next to an icon. The band is measured from a fixed reference run, which makes the offset a property of the font at this size and shared by every label in the style — centring each string on its own ink moved it by whether it held a descender.
        let (ink_top, ink_height, reference_line) = {
            let key = style.font_size.to_bits();
            let mut cache = self.cached_ink.borrow_mut();
            match cache.as_ref() {
                Some((k, top, h, line)) if *k == key => (*top, *h, *line),
                _ => {
                    let (top, h) =
                        crate::text_metrics::measure_ink_bounds(REFERENCE, REFERENCE_WIDTH, &style);
                    let (_, line) =
                        crate::text_metrics::measure_text(REFERENCE, None, REFERENCE_WIDTH, &style);
                    *cache = Some((key, top, h, line));
                    (top, h, line)
                }
            }
        };
        // Centre the whole block, then nudge by how far the band sits off the middle of one line box. The nudge is a property of the font, so it applies once however many lines there are; centring against the band alone would push an N-line block down by (N-1)/2 lines.
        let (_, text_height) = crate::text_metrics::measure_text(&text, None, r.width, &style);
        let nudge = if ink_height > 0.0 {
            reference_line / 2.0 - ink_top - ink_height / 2.0
        } else {
            0.0
        };
        // Snapped to the pixel grid: centring lands on a half pixel whenever box and text differ by an odd amount, and a glyph drawn half a row down is resampled across two rows and goes soft. Only the vertical axis — horizontal subpixel placement is what keeps letter spacing even. Clamped within the leaf, since a box no taller than one line has nothing to centre in and the nudge would walk glyphs out through its top.
        let slack = (r.height - text_height).max(0.0);
        let y = ((slack / 2.0 + nudge).clamp(0.0, slack)).round();
        // The full line box, so nothing clips.
        let line_height = text_height;
        let spans: Option<std::sync::Arc<[Span]>> =
            self.spans.as_ref().map(|f| std::sync::Arc::from(f()));
        self.leaf.at_layout_position(RenderNode::spanned_text(
            text,
            spans.unwrap_or_else(|| std::sync::Arc::from([].as_slice())),
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

#[cfg(test)]
#[path = "text_test.rs"]
mod tests;
