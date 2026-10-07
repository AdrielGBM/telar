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
use crate::text_fit::Fitting;

/// What a text was last measured from: its string, its declared style, and the fit it asked for with the surface that fit read.
type Measured = (
    String,
    TextStyle,
    Option<(renderer_core::FontFit, geometry_core::Size)>,
);

/// The run the glyph band is measured from: a capital, an x-height letter and a descender, which between them span the extent a Latin face actually draws in. Any string of the same style is then centred by the same amount, which is what puts a row of labels on one baseline.
const REFERENCE: &str = "Hxg";
/// Room the reference cannot fill: it is three characters, and cosmic-text overflows on an unbounded one.
const REFERENCE_WIDTH: f32 = 1_000.0;

/// The measured text leaf, optically centred in the box its parent gave it.
type InkKey = (u32, u64);

pub struct Text {
    content: Rc<dyn Fn() -> String>,
    // The byte ranges that restyle themselves, or `None` for a paragraph without any.
    spans: Option<Rc<dyn Fn() -> Vec<Span>>>,
    cached_content: RefCell<(String, Arc<str>)>,
    // (font_size bits, text metrics generation) -> (ink_top, ink_height, line_height). Keyed on size and not on text, because the band is measured from a reference run.
    cached_ink: RefCell<Option<(InkKey, f32, f32, f32)>>,
    style: Rc<dyn Fn() -> TextStyle>,
    leaf: LayoutLeaf,
    // Held for its subscription: without it a measured leaf keeps the width the previous string wanted, and a label that grew soft-wraps into a slot built for the old text.
    _remeasure: Option<Effect>,
    pressed_run: Option<(usize, (f32, f32))>,
    link_cursor: Option<crate::cursor::CursorClaim>,
}

/// One run of a paragraph built with [`Text::runs`]: its string, what it declares over the paragraph's style, and where it links.
pub struct TextRun {
    content: Rc<dyn Fn() -> String>,
    over: Option<Rc<dyn Fn() -> renderer_core::Declared>>,
    link: Option<Rc<dyn Fn() -> Option<platform_core::Destination>>>,
}

impl TextRun {
    pub fn new(content: impl Fn() -> String + 'static) -> Self {
        Self {
            content: Rc::new(content),
            over: None,
            link: None,
        }
    }

    /// Restyles the run: what `over` declares wins over the paragraph's style for its stretch, re-read when what it reads changes.
    pub fn declaring(mut self, over: impl Fn() -> renderer_core::Declared + 'static) -> Self {
        self.over = Some(Rc::new(over));
        self
    }

    /// Makes the run a link, the way [`StyledContainer::to`](crate::StyledContainer::to) makes a box one: a route, an anchor or an external URI, or an `Option` of one for a link with nowhere to go right now.
    pub fn to<D: platform_core::IntoDestination>(
        mut self,
        destination: impl Fn() -> D + 'static,
    ) -> Self {
        self.link = Some(Rc::new(move || destination().into_destination()));
        self
    }
}

/// Where a text gets its style: given whole, derived from what the tree above it declared, or derived and then fitted to a width.
enum StyleSource {
    Complete(Rc<dyn Fn() -> TextStyle>),
    Inheriting(Rc<dyn Fn(TextStyle) -> TextStyle>),
    Fitting(
        Rc<dyn Fn(TextStyle) -> TextStyle>,
        Rc<dyn Fn() -> renderer_core::FontFit>,
    ),
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

    /// [`declaring`](Self::declaring) with its size found rather than given: the size that sets its one line to the width `fit_fn` asks for, resolved by layout where it knows the box holding the text. `style_fn` is handed the size being tried, so what it declares in `em` scales with the line. See [`FontFit`](renderer_core::FontFit).
    pub fn fitting(
        content_fn: impl Fn() -> String + 'static,
        layout_style: LayoutStyle,
        fit_fn: impl Fn() -> renderer_core::FontFit + 'static,
        style_fn: impl Fn(TextStyle) -> TextStyle + 'static,
    ) -> Result<Self, LayoutError> {
        Self::build(
            Rc::new(content_fn),
            None,
            layout_style,
            StyleSource::Fitting(Rc::new(style_fn), Rc::new(fit_fn)),
        )
    }

    /// [`new`](Self::new) with byte ranges that style themselves differently from the paragraph — a bold word, a coloured link — shaped and wrapped as one text rather than as separate widgets. [`spanned_declaring`](Self::spanned_declaring) is the form that inherits.
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

    /// [`spanned`](Self::spanned) styled by what the tree above it declared, amended by `style_fn`, like [`declaring`](Self::declaring): each span declares over the inherited style, so the paragraph and its spans follow the surface's family and the sizes around them.
    ///
    /// [`runs`](Self::runs) is the same paragraph written as a fixed list of strings. This is the form for spans a string is scanned for — matches, mentions, highlighted tokens — whose number moves with the text.
    pub fn spanned_declaring(
        content_fn: impl Fn() -> String + 'static,
        spans_fn: impl Fn() -> Vec<Span> + 'static,
        layout_style: LayoutStyle,
        style_fn: impl Fn(TextStyle) -> TextStyle + 'static,
    ) -> Result<Self, LayoutError> {
        Self::build(
            Rc::new(content_fn),
            Some(Rc::new(spans_fn)),
            layout_style,
            StyleSource::Inheriting(Rc::new(style_fn)),
        )
    }

    /// A paragraph written as runs, each its own string and each free to restyle itself or link somewhere: what `text` with `span` children builds. Shaped, wrapped and measured as one text, styled by what the tree above declared amended by `style_fn`, like [`declaring`](Self::declaring).
    pub fn runs(
        runs: Vec<TextRun>,
        layout_style: LayoutStyle,
        style_fn: impl Fn(TextStyle) -> TextStyle + 'static,
    ) -> Result<Self, LayoutError> {
        Self::from_runs(
            runs,
            layout_style,
            StyleSource::Inheriting(Rc::new(style_fn)),
        )
    }

    /// [`runs`](Self::runs) fitted to a width, as [`fitting`](Self::fitting) is.
    pub fn runs_fitting(
        runs: Vec<TextRun>,
        layout_style: LayoutStyle,
        fit_fn: impl Fn() -> renderer_core::FontFit + 'static,
        style_fn: impl Fn(TextStyle) -> TextStyle + 'static,
    ) -> Result<Self, LayoutError> {
        Self::from_runs(
            runs,
            layout_style,
            StyleSource::Fitting(Rc::new(style_fn), Rc::new(fit_fn)),
        )
    }

    fn from_runs(
        runs: Vec<TextRun>,
        layout_style: LayoutStyle,
        source: StyleSource,
    ) -> Result<Self, LayoutError> {
        let runs: Rc<[TextRun]> = runs.into();
        let content = {
            let runs = runs.clone();
            Rc::new(move || runs.iter().map(|run| (run.content)()).collect::<String>())
        };
        let spans = Rc::new(move || {
            let mut at = 0u32;
            let mut spans = Vec::new();
            for run in runs.iter() {
                let start = at;
                at += (run.content)().len() as u32;
                if run.over.is_none() && run.link.is_none() {
                    continue;
                }
                let over = run.over.as_ref().map(|over| over()).unwrap_or_default();
                let mut span = Span::new(start..at, over);
                span.link = run.link.as_ref().and_then(|link| link());
                spans.push(span);
            }
            spans
        });
        Self::build(content, Some(spans), layout_style, source)
    }

    fn build(
        content_fn: Rc<dyn Fn() -> String>,
        spans_fn: Option<Rc<dyn Fn() -> Vec<Span>>>,
        layout_style: LayoutStyle,
        source: StyleSource,
    ) -> Result<Self, LayoutError> {
        // The node does not exist until the leaf is registered, and that call's measure closure already reads the style, so the cell lets the style close over a node older than itself.
        let node_cell = Rc::new(std::cell::Cell::new(None::<layout_core::NodeId>));
        let in_language = |styled: Rc<dyn Fn() -> TextStyle>| -> Rc<dyn Fn() -> TextStyle> {
            let cell = Rc::clone(&node_cell);
            Rc::new(move || {
                let style = styled();
                if style.lang.is_some() {
                    return style;
                }
                let lang = match cell.get() {
                    Some(node) => crate::annotation::language_at(node),
                    None => i18n_core::use_locale().map(Arc::from),
                };
                TextStyle { lang, ..style }
            })
        };
        let inherited = in_language({
            let cell = Rc::clone(&node_cell);
            Rc::new(move || crate::inherit::inherited_text_style_at(cell.get()))
        });
        let declared_by = |amend: Rc<dyn Fn(TextStyle) -> TextStyle>| {
            let inherited = Rc::clone(&inherited);
            Rc::new(move || amend(inherited())) as Rc<dyn Fn() -> TextStyle>
        };
        let (declared, fitting) = match source {
            StyleSource::Complete(style_fn) => (in_language(style_fn), None),
            StyleSource::Inheriting(amend) => (declared_by(amend), None),
            StyleSource::Fitting(amend, fit) => (
                declared_by(Rc::clone(&amend)),
                Some(Rc::new(Fitting::new(
                    amend,
                    fit,
                    Rc::clone(&inherited),
                    Rc::clone(&node_cell),
                ))),
            ),
        };
        let style: Rc<dyn Fn() -> TextStyle> = match &fitting {
            Some(fitting) => {
                let fitting = Rc::clone(fitting);
                Rc::new(move || fitting.drawn_style())
            }
            None => Rc::clone(&declared),
        };

        let measure_content = Rc::clone(&content_fn);
        let measure_style = Rc::clone(&declared);
        let measure_spans = spans_fn.clone();
        let measure_fitting = fitting.clone();
        let measure = Box::new(move |input: layout_core::MeasureInput| {
            let content = (measure_content)();
            // Spans change the extent, so a box measured without them wraps differently from the text drawn into it.
            let spans = measure_spans.as_ref().map(|f| f());
            let s = match &measure_fitting {
                Some(fitting) => {
                    fitting.measured_style(&content, spans.as_deref(), input.containing_width)
                }
                None => (measure_style)(),
            };
            crate::text_metrics::measure_in(&content, spans.as_deref(), input.width, &s)
        });

        // Stretch overrides any parent align-items, so text fills the cross axis instead of collapsing to 0.
        let (node, rect) =
            crate::context::new_measured_leaf(layout_style.align_self_stretch(), measure)?;
        node_cell.set(Some(node));
        // Reads what the measure reads, so it subscribes to exactly what the measure depends on, and compares before dirtying: a signal re-set to its own value, or a colour change, would otherwise cost a shaping pass and a relayout for nothing.
        let dirty_content = Rc::clone(&content_fn);
        let dirty_style = Rc::clone(&declared);
        let dirty_fit = fitting;
        let measured = RefCell::new(Option::<Measured>::None);
        let remeasure = effect(move || {
            let next = (
                (dirty_content)(),
                (dirty_style)(),
                dirty_fit.as_ref().map(|fitting| fitting.watched()),
            );
            let unchanged = measured
                .borrow()
                .as_ref()
                .is_some_and(|(content, style, fit)| {
                    *content == next.0 && style.same_extent(&next.1) && *fit == next.2
                });
            if unchanged {
                return;
            }
            *measured.borrow_mut() = Some(next);
            mark_dirty(node).ok();
        });
        if let Some(spans) = spans_fn.clone() {
            crate::link::register_runs(
                node,
                Rc::new(move |run| spans().get(run).and_then(|span| span.link.clone())),
            );
        }
        Ok(Self {
            content: content_fn,
            spans: spans_fn,
            cached_content: RefCell::new((String::new(), Arc::from(""))),
            cached_ink: RefCell::new(None),
            style,
            leaf: LayoutLeaf { node, rect },
            _remeasure: Some(remeasure),
            pressed_run: None,
            link_cursor: None,
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

impl Text {
    /// Where the text sits inside a box `r`, as `(top, height)`: optically centred, see the comments inside.
    fn placed(&self, r: Rect, text: &str, style: &TextStyle) -> (f32, f32) {
        // A text leaf stretches to fill its parent's cross axis, and the font's line box reserves ascent room a run never uses, so line-box-centred text sits visibly high next to an icon. The band is measured from a fixed reference run, which makes the offset a property of the font at this size and shared by every label in the style — centring each string on its own ink moved it by whether it held a descender.
        let (ink_top, ink_height, reference_line) = {
            let key = (
                style.font_size.to_bits(),
                renderer_core::text_metrics_generation(),
            );
            let mut cache = self.cached_ink.borrow_mut();
            match cache.as_ref() {
                Some((k, top, h, line)) if *k == key => (*top, *h, *line),
                _ => {
                    let (top, h) =
                        crate::text_metrics::measure_ink_bounds(REFERENCE, REFERENCE_WIDTH, style);
                    let (_, line) =
                        crate::text_metrics::measure_text(REFERENCE, None, REFERENCE_WIDTH, style);
                    *cache = Some((key, top, h, line));
                    (top, h, line)
                }
            }
        };
        // Centre the whole block, then nudge by how far the band sits off the middle of one line box. The nudge is a property of the font, so it applies once however many lines there are; centring against the band alone would push an N-line block down by (N-1)/2 lines.
        let (_, text_height) = crate::text_metrics::measure_text(text, None, r.width, style);
        let nudge = if ink_height > 0.0 {
            reference_line / 2.0 - ink_top - ink_height / 2.0
        } else {
            0.0
        };
        // Snapped to the pixel grid: centring lands on a half pixel whenever box and text differ by an odd amount, and a glyph drawn half a row down is resampled across two rows and goes soft. Only the vertical axis — horizontal subpixel placement is what keeps letter spacing even. Clamped within the leaf, since a box no taller than one line has nothing to centre in and the nudge would walk glyphs out through its top.
        let slack = (r.height - text_height).max(0.0);
        let y = ((slack / 2.0 + nudge).clamp(0.0, slack)).round();
        (y, text_height)
    }

    /// The link span under `(x, y)`, with its position among the spans, found where the text is drawn.
    fn link_under(&self, x: f32, y: f32) -> Option<(usize, platform_core::Destination)> {
        let spans = (self.spans.as_ref()?)();
        if spans.iter().all(|span| span.link.is_none()) {
            return None;
        }
        let r = self.leaf.rect.get();
        if !r.contains(x, y) {
            return None;
        }
        let text = (self.content)();
        let style = (self.style)();
        let (top, _) = self.placed(r, &text, &style);
        let index = renderer_core::text_index_at(
            &text,
            Some(&spans),
            r.width,
            &style,
            (x - r.x, y - r.y - top),
        )?;
        renderer_core::link_at(&spans, index).map(|(run, link)| (run, link.clone()))
    }

    /// A press, a drag and a release over the link spans: a tap on one follows it, and the pointer takes the link shape over one. Over a span something else is drawn in front of — a sibling on top, or a part a viewport has scrolled out of view — neither happens ([`crate::pointer::pointer_occluded`]). A document answers its own `<a>` and reports it back instead (see [`Event::RunActivated`]).
    fn follow_runs(&mut self, event: &Event) -> EventResult {
        if self.spans.is_none() || crate::link::surface_follows_links() {
            return EventResult::Ignored;
        }
        match event {
            Event::PointerMoved { x, y, source, .. } => {
                let (x, y) = (*x as f32, *y as f32);
                if let Some((_, (ox, oy))) = self.pressed_run
                    && (x - ox).powi(2) + (y - oy).powi(2) > platform_core::TAP_SLOP.powi(2)
                {
                    self.pressed_run = None;
                }
                if matches!(source, platform_core::PointerSource::Mouse) {
                    let over =
                        !crate::pointer::pointer_occluded() && self.link_under(x, y).is_some();
                    if over || self.link_cursor.is_some() {
                        self.link_cursor
                            .get_or_insert_with(|| {
                                crate::cursor::CursorClaim::new(platform_core::Cursor::Pointer)
                            })
                            .hover(crate::cursor::depth(), over);
                    }
                }
                EventResult::Ignored
            }
            Event::PointerPressed {
                x,
                y,
                button: platform_core::PointerButton::Primary,
                ..
            } => match self.link_under(*x as f32, *y as f32) {
                Some((run, _)) => {
                    self.pressed_run = Some((run, (*x as f32, *y as f32)));
                    EventResult::Handled
                }
                None => EventResult::Ignored,
            },
            Event::PointerReleased {
                x,
                y,
                button: platform_core::PointerButton::Primary,
                ..
            } => {
                let Some((pressed, _)) = self.pressed_run.take() else {
                    return EventResult::Ignored;
                };
                if crate::pointer::pointer_occluded() {
                    return EventResult::Ignored;
                }
                match self.link_under(*x as f32, *y as f32) {
                    Some((run, destination)) if run == pressed => {
                        crate::link::follow_pressed(&destination, crate::modifiers());
                        EventResult::Handled
                    }
                    _ => EventResult::Ignored,
                }
            }
            _ => EventResult::Ignored,
        }
    }
}

impl Drop for Text {
    fn drop(&mut self) {
        if self.spans.is_some() {
            crate::link::unregister_runs(self.leaf.node);
        }
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
        let (y, line_height) = self.placed(r, &text, &style);
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

    fn on_event(&mut self, event: &Event) -> EventResult {
        self.follow_runs(event)
    }

    fn debug_name(&self) -> &'static str {
        "Text"
    }
}

impl_leaf_widget!(Text);

#[cfg(test)]
#[path = "text_test.rs"]
mod tests;
