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

/// The run the glyph band is measured from: a capital, an x-height letter and a descender, which between
/// them span the extent a Latin face actually draws in. Any string of the same style is then centred by the
/// same amount, which is what puts a row of labels on one baseline.
const REFERENCE: &str = "Hxg";
/// Room the reference cannot fill: it is three characters, and cosmic-text overflows on an unbounded one.
const REFERENCE_WIDTH: f32 = 1_000.0;

pub struct Text {
    content: Rc<dyn Fn() -> String>,
    // The byte ranges that restyle themselves, or `None` for a paragraph that does not have any.
    spans: Option<Rc<dyn Fn() -> Vec<Span>>>,
    cached_content: RefCell<(String, Arc<str>)>,
    // Glyph-band memo for optical vertical centering: font_size bits -> (ink_top, ink_height, line_height).
    // Keyed on the size and not on the text, because the band is measured from a reference run — see `view`.
    cached_ink: RefCell<Option<(u32, f32, f32, f32)>>,
    style: Rc<dyn Fn() -> TextStyle>,
    leaf: LayoutLeaf,
    // Held for its subscription: without it a measured leaf keeps the width the previous string wanted, and `view` shapes the new one into that box — a label that grew soft-wraps into a slot built for the old text.
    _remeasure: Option<Effect>,
}

/// Where a text gets its style: given whole, or derived from what the tree above it declared.
enum StyleSource {
    Complete(Rc<dyn Fn() -> TextStyle>),
    Inheriting(Rc<dyn Fn(TextStyle) -> TextStyle>),
}

impl Text {
    /// A text leaf whose height is measured from its content at the resolved width, so the box grows to fit
    /// however many lines the text wraps into and pushes following siblings down instead of overflowing.
    ///
    /// There is no fixed-height counterpart: an explicit `height` in `layout_style` pins the box, which is
    /// what the second constructor was for. Choosing between them by whether the markup happened to write a
    /// height meant the same label measured or did not depending on a detail of how it was asked for.
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
    /// The amendment takes the inherited style and returns the final one, rather than naming properties,
    /// because a leaf has two kinds of thing to say: an override of something inherited (`font_size`) and a
    /// clamp that could never be inherited (`max_lines`). One closure carries both, and it is the shape a
    /// caller already amends a `RectStyle` with.
    ///
    /// [`new`](Self::new) is the opt-out: passing a whole `TextStyle` is the honest way to say the tree above
    /// has no business in this one.
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

    /// [`new`](Self::new) with byte ranges that style themselves differently from the paragraph — a bold
    /// word, a coloured link — shaped and wrapped as one text rather than as separate widgets.
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
        // The node does not exist until the leaf is registered, and the measure closure handed to that call already reads the style: a cell filled the moment the node exists lets the style close over a node older than itself.
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
            // Measured with its spans, because they change the extent: a box measured without them wraps differently from the text drawn into it.
            let spans = measure_spans.as_ref().map(|f| f());
            crate::text_metrics::measure_text(&(measure_content)(), spans.as_deref(), max_width, &s)
        });

        // Stretch overrides any parent align-items (e.g. center) so text always fills the parent's cross-axis width instead of collapsing to 0.
        let (node, rect) =
            crate::context::new_measured_leaf(layout_style.align_self_stretch(), measure)?;
        node_cell.set(Some(node));
        // Reads through the measure closure so it subscribes to exactly the signals the measure depends on, and keeps the string it last dirtied for: a signal re-set to its own value would otherwise cost a shaping pass and a relayout of the surface for nothing.
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
        // Optically center the glyph band within the leaf. A text leaf stretches to fill its parent's cross
        // axis (`align_self_stretch`), and the font's line box reserves ascent room for accents that a run
        // never uses — so line-box-centered text sits visibly high next to an icon.
        //
        // The band is measured from a fixed REFERENCE run and not from this text, and that distinction is
        // the whole point: it makes the offset a property of the *font at this size*, which every label in
        // the same style then shares. Centering each string on its own ink instead moved it by whether it
        // happened to contain a descender — so `Modeling` and `Setup` sat on one baseline and `Simulation`
        // and `Results` sat 1.5px below it, in the same row of tabs. A row of labels that does not share a
        // baseline is the kind of wrong that is obvious once seen and invisible until then.
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
        // Centre the whole block, then nudge it by how far the glyph band sits off the middle of **one**
        // line box. Splitting it that way is what makes it work for more than one line: the nudge is a
        // property of the font at this size, so it applies once however many lines there are, while
        // centring against the band alone would push an N-line block down by (N-1)/2 lines — which is what
        // a two-line tooltip did, sinking its second line out of the bubble.
        let (_, text_height) = crate::text_metrics::measure_text(&text, None, r.width, &style);
        let nudge = if ink_height > 0.0 {
            reference_line / 2.0 - ink_top - ink_height / 2.0
        } else {
            0.0
        };
        // Rounded onto the pixel grid. Centring lands on a half pixel whenever the box and the text differ
        // by an odd amount, and a glyph drawn half a row down is resampled across two rows: it does not move,
        // it goes **soft**. Which is why it looked like a placement bug — the same bubble was crisp beside a
        // button and blurred under one, because the sideways placement centres on the trigger and contributed
        // its own half pixel, cancelling this one. Vertical position is the axis to snap; horizontal subpixel
        // placement is what keeps letter spacing even, and the shaper bins it on purpose.
        // Within the leaf, always. The nudge is a *centring* refinement, and a box no taller than one line
        // has nothing to centre in: applying it there walks the glyphs out through the top of the box the
        // layout reserved for them, so a `pad:6` label inked at row 5.
        let slack = (r.height - text_height).max(0.0);
        let y = ((slack / 2.0 + nudge).clamp(0.0, slack)).round();
        // Render the full line box so nothing clips.
        let line_height = text_height;
        let spans: Option<std::sync::Arc<[Span]>> =
            self.spans.as_ref().map(|f| std::sync::Arc::from(f()));
        let placed = self.leaf.at_layout_position(RenderNode::spanned_text(
            text,
            spans.unwrap_or_else(|| std::sync::Arc::from([].as_slice())),
            Rect {
                x: 0.0,
                y,
                width: r.width,
                height: line_height,
            },
            style,
        ));
        // A paragraph is a box of its own to layout, so it has to be one to a document backend too:
        // dropping the text straight into its parent would lose the width and the flex share it was given.
        if ui_tree::element_capture() {
            let layout = layout_reactive::declared_css(self.leaf.node)
                .map(|css| css.into_string())
                .unwrap_or_default();
            RenderNode::element(
                std::sync::Arc::new(renderer_core::Element::new(
                    renderer_core::ElementId(self.leaf.node.into()),
                    renderer_core::Semantics::group(),
                    layout,
                )),
                [placed],
            )
        } else {
            placed
        }
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
mod tests {
    use super::*;
    use crate::context::{
        compute_layout, new_container, relayout_if_dirty, reset_layout_runtime, track_layout,
    };
    use crate::layout_item::LayoutItem;
    use layout_core::AvailableSpace;
    use reactive_core::signal;
    use renderer_core::Color;

    // Auto-height text must reserve more vertical space when it is narrower (more wrapped lines), so
    // following content is pushed down instead of overlapped.
    #[test]
    fn auto_text_height_grows_when_narrower() {
        let long = "This is a deliberately long paragraph of text that wraps onto several \
                    lines when the available width is small, and fewer lines when it is wide.";
        let height_at = |w: f32| -> f32 {
            reset_layout_runtime();
            let t = Text::new(
                move || long.to_string(),
                LayoutStyle::new(),
                || TextStyle::new(16.0, Color::BLACK),
            )
            .unwrap();
            let node = t.layout_node();
            compute_layout(
                node,
                AvailableSpace::Definite(w),
                AvailableSpace::MaxContent,
            )
            .unwrap();
            track_layout(node).unwrap().get().height
        };
        let narrow = height_at(200.0);
        let wide = height_at(800.0);
        assert!(
            narrow > wide + 20.0,
            "narrow text should be taller: narrow={narrow} wide={wide}"
        );
    }

    /// Two labels of the same style sit on the same baseline, whatever letters they happen to contain.
    ///
    /// They did not: the optical centring measured each string's own ink, and a string with a descender has
    /// ink reaching lower than one without — so `Setup` and `Simulation`, side by side in a row of tabs at
    /// the same size in the same box, were drawn 2.5px apart. Measuring the band from a reference run makes
    /// the offset a property of the font at that size, which every label in the style then shares.
    #[test]
    fn two_labels_of_one_style_share_a_baseline_whatever_letters_they_have() {
        reset_layout_runtime();
        let drawn = |content: &'static str| {
            let text = Text::new(
                move || content.to_string(),
                LayoutStyle::new().width(200.0).height(30.0),
                || TextStyle::new(13.0, Color::BLACK),
            )
            .unwrap();
            let root = new_container(
                LayoutStyle::new().flex_column().width(200.0).height(30.0),
                &[text.layout_node()],
            )
            .unwrap();
            compute_layout(
                root,
                AvailableSpace::Definite(200.0),
                AvailableSpace::Definite(30.0),
            )
            .unwrap();
            // The leaf places itself with a transform, so the text command sits under it.
            fn text_y(node: &RenderNode) -> Option<f32> {
                match node {
                    RenderNode::Primitive(renderer_core::DrawCommand::Text { rect, .. }) => {
                        Some(rect.y)
                    }
                    RenderNode::Transform { children, .. } | RenderNode::Group { children } => {
                        children.iter().find_map(text_y)
                    }
                    _ => None,
                }
            }
            text_y(&text.view()).expect("a text leaf draws text")
        };
        // `Setup` has a descender and `Simulation` has none — the exact pair that drifted.
        let (with_tail, without) = (drawn("Setup"), drawn("Simulation"));
        assert!(
            (with_tail - without).abs() < 0.01,
            "a descender must not move the line: {with_tail} vs {without}"
        );
    }

    /// Text lands on a whole pixel row, whatever its box measures.
    ///
    /// Centring puts it on a half pixel whenever the box and the text differ by an odd amount, and a glyph
    /// drawn half a row down does not move — it is resampled across two rows and goes **soft**. It read as a
    /// *placement* bug: the same bubble was crisp beside a button and blurred under one, because the sideways
    /// placement centres on its trigger and happened to contribute a second half pixel that cancelled this
    /// one. Only the vertical axis is snapped; horizontal subpixel placement is what keeps letter spacing
    /// even, and the shaper bins it deliberately.
    #[test]
    fn text_lands_on_a_whole_pixel_row() {
        fn text_y(node: &RenderNode) -> Option<f32> {
            match node {
                RenderNode::Primitive(renderer_core::DrawCommand::Text { rect, .. }) => {
                    Some(rect.y)
                }
                RenderNode::Transform { children, .. } | RenderNode::Group { children } => {
                    children.iter().find_map(text_y)
                }
                _ => None,
            }
        }
        reset_layout_runtime();
        // Odd and fractional box heights, where an unsnapped centre lands on the half.
        for height in [29.0_f32, 30.0, 31.0, 44.5] {
            let text = Text::new(
                || "Setup".to_string(),
                LayoutStyle::new().width(200.0).height(height),
                || TextStyle::new(13.0, Color::BLACK),
            )
            .unwrap();
            let root = new_container(
                LayoutStyle::new().flex_column().width(200.0).height(height),
                &[text.layout_node()],
            )
            .unwrap();
            compute_layout(
                root,
                AvailableSpace::Definite(200.0),
                AvailableSpace::Definite(height),
            )
            .unwrap();
            let y = text_y(&text.view()).expect("a text leaf draws text");
            assert_eq!(y, y.round(), "a {height}px box put the text at {y}");
        }
    }

    /// The optical nudge never walks the glyphs out of the box the layout reserved for them.
    ///
    /// The nudge centres the glyph band, and a box no taller than one line has nothing to centre in — so
    /// applying it there put the text at a negative `y`. A `pad:6` status line then inked at row 5, one row
    /// above its own padding, which is how an out-of-tree app found this.
    #[test]
    fn text_stays_inside_a_box_that_is_exactly_one_line_tall() {
        fn text_rect(node: &RenderNode) -> Option<Rect> {
            match node {
                RenderNode::Primitive(renderer_core::DrawCommand::Text { rect, .. }) => Some(*rect),
                RenderNode::Transform { children, .. } | RenderNode::Group { children } => {
                    children.iter().find_map(text_rect)
                }
                _ => None,
            }
        }
        reset_layout_runtime();
        for size in [11.0_f32, 13.0, 15.0, 24.0] {
            let text = Text::new(
                || "Ag".to_string(),
                LayoutStyle::new().width(200.0),
                move || TextStyle::new(size, Color::BLACK),
            )
            .unwrap();
            let root = new_container(
                LayoutStyle::new().flex_column().width(200.0),
                &[text.layout_node()],
            )
            .unwrap();
            compute_layout(
                root,
                AvailableSpace::Definite(200.0),
                AvailableSpace::MaxContent,
            )
            .unwrap();
            let rect = text_rect(&text.view()).expect("a text leaf draws text");
            assert!(
                rect.y >= 0.0,
                "at {size}px the text starts {}px above its own box",
                -rect.y
            );
        }
    }

    /// A block that wraps sits where its box is, not half a line below it.
    ///
    /// Optical centring works on the glyph band, and the band is measured from a one-line reference — so
    /// centring an N-line block against it pushes the block down by (N-1)/2 lines. A two-line tooltip
    /// description came out sunk, with its second line hanging out of the bubble. Centring the *block* and
    /// nudging by the one-line correction is what makes the two cases the same case.
    #[test]
    fn a_wrapped_block_is_not_pushed_down_by_the_lines_it_gained() {
        reset_layout_runtime();
        let style = || TextStyle::new(13.0, Color::BLACK);
        let text = Text::new(
            || "Name regions and say what the model is made of".to_string(),
            LayoutStyle::new(),
            style,
        )
        .unwrap();
        let root = new_container(
            LayoutStyle::new().flex_column().width(150.0),
            &[text.layout_node()],
        )
        .unwrap();
        compute_layout(
            root,
            AvailableSpace::Definite(150.0),
            AvailableSpace::MaxContent,
        )
        .unwrap();

        let (_, one_line) = crate::text_metrics::measure_text("Hxg", None, 1_000.0, &style());
        let (_, block) = crate::text_metrics::measure_text(
            "Name regions and say what the model is made of",
            None,
            150.0,
            &style(),
        );
        assert!(
            block > one_line * 1.5,
            "the test needs a string that actually wraps"
        );

        fn text_y(node: &RenderNode) -> Option<f32> {
            match node {
                RenderNode::Primitive(renderer_core::DrawCommand::Text { rect, .. }) => {
                    Some(rect.y)
                }
                RenderNode::Transform { children, .. } | RenderNode::Group { children } => {
                    children.iter().find_map(text_y)
                }
                _ => None,
            }
        }
        let y = text_y(&text.view()).expect("a text leaf draws text");
        assert!(
            y.abs() < one_line / 3.0,
            "a block in a box its own size starts at the top: y = {y} against a {one_line}px line"
        );
    }

    /// A label that grows re-measures, instead of being shaped into the width the previous string wanted.
    ///
    /// The regression it guards is invisible in the widget tree and obvious on screen: a measured leaf is
    /// dirtied by the layout runtime, never by a content closure, so a bar chip whose title went from
    /// "Desktop" to a full window title kept the narrow box the short one had measured — and `view` soft-wrapped
    /// the long title into it, spilling several lines out of a chip one line tall.
    #[test]
    fn a_measured_label_re_measures_when_its_content_changes() {
        reset_layout_runtime();
        let title = signal(String::from("Desktop"));
        let read = title.read_only();
        let label = Text::new(
            move || read.get(),
            LayoutStyle::new(),
            || TextStyle::new(13.0, Color::BLACK),
        )
        .unwrap();
        let node = label.layout_node();
        let root = new_container(
            LayoutStyle::new().flex_row().width(1920.0).height(32.0),
            &[node],
        )
        .unwrap();
        let space = || {
            compute_layout(
                root,
                AvailableSpace::Definite(1920.0),
                AvailableSpace::Definite(32.0),
            )
            .unwrap()
        };

        space();
        let short = label.leaf.rect.get().width;

        title.set("hyprshell - Rust - Visual Studio Code".to_string());
        relayout_if_dirty();
        let long = label.leaf.rect.get().width;

        assert!(
            long > short,
            "a title five times longer still measured {long}px, the width \"Desktop\" wanted ({short}px) — \
             it will be wrapped into a box built for the old text"
        );
    }
}
