//! Layout-time text measurement, through the measurer the active surface lays its text out with.
//!
//! Per surface, like its locale and its default family: a window drawn in glyphs and a canvas drawn in a terminal's cells measure the same string differently, and each surface's renderer says which way its text is drawn.

use std::sync::Arc;

use layout_core::AvailableSpace;
use reactive_core::{RwSignal, in_surface_world, signal};
use renderer_core::{Color, Span, TextLine, TextMetrics, TextStyle};

#[derive(Clone)]
struct Measurer {
    metrics: Arc<dyn TextMetrics>,
    /// One past the generation the measurer it replaced had reached, so a replacement reads as a change whatever the two count from.
    base: u64,
    opened_at: u64,
}

impl Measurer {
    fn generation(&self) -> u64 {
        self.base + self.metrics.generation().saturating_sub(self.opened_at)
    }
}

#[derive(Default)]
struct SurfaceMetrics {
    measurer: Option<Measurer>,
    generation: Option<RwSignal<u64>>,
}

reactive_core::surface_local! {
    /// The active surface's measurer, and the generation its widgets follow.
    slot SURFACE_METRICS: SurfaceMetrics = SurfaceMetrics::default();
    access with_surface_metrics, with_surface_metrics_ref;
    context TextMetricsContext;
}

/// Hands the active surface the measurer its text is laid out with: the one the renderer that draws it measures with, [`RendererFactory::text_metrics`](renderer_core::RendererFactory::text_metrics). The runner calls this before it builds the surface's tree; a test or a tool that lays a tree out with no runner behind it calls it to stand in for one.
///
/// A measurer opened over another one measures every text on the surface again at its next layout.
pub fn open_surface_text_metrics(metrics: Arc<dyn TextMetrics>) {
    let opened_at = metrics.generation();
    with_surface_metrics(|slot| {
        let base = slot
            .measurer
            .as_ref()
            .map_or(0, |measurer| measurer.generation() + 1);
        slot.measurer = Some(Measurer {
            metrics,
            base,
            opened_at,
        });
    });
}

/// The measurer the active surface lays its text out with, `None` until one is opened.
pub fn surface_text_metrics() -> Option<Arc<dyn TextMetrics>> {
    with_surface_metrics_ref(|slot| {
        slot.measurer
            .as_ref()
            .map(|measurer| Arc::clone(&measurer.metrics))
    })
}

// A unit test that lays out a label measures with the shaper unless it opened a measurer of its own, so it need not name one.
#[cfg(test)]
fn open_in_tests() {
    if surface_text_metrics().is_none() {
        open_surface_text_metrics(Arc::new(renderer_text::ShaperMetrics));
    }
}

fn metrics() -> Arc<dyn TextMetrics> {
    #[cfg(test)]
    open_in_tests();
    surface_text_metrics().expect(
        "the active surface has no TextMetrics, so nothing can size its text: the runner opens its \
         renderer's with ui_core::open_surface_text_metrics before building the tree, and code with \
         no runner opens one itself (telar::open_default_text_metrics opens the glyph shaper's)",
    )
}

/// The logical `(width, height)` of `text` wrapped to `max_width` on the active surface. See [`TextMetrics::measure`].
pub fn measure_text(
    text: &str,
    spans: Option<&[Span]>,
    max_width: f32,
    style: &TextStyle,
) -> (f32, f32) {
    metrics().measure(text, spans, max_width, style)
}

/// The `(width, height)` a text takes in the width layout offers it: wrapped to a definite width, on one line for its max-content size, and at its longest word for its min-content size.
pub(crate) fn measure_in(
    text: &str,
    spans: Option<&[Span]>,
    width: AvailableSpace,
    style: &TextStyle,
) -> (f32, f32) {
    match width {
        AvailableSpace::Definite(width) => measure_text(text, spans, width, style),
        AvailableSpace::MaxContent => measure_text(text, spans, MAX_CONTENT_WIDTH, style),
        AvailableSpace::MinContent => metrics().min_content(text, spans, style),
    }
}

/// Wide enough that nothing wraps, and finite, since a measurer turns the width into whole pixels or cells.
const MAX_CONTENT_WIDTH: f32 = 1.0e6;

pub(crate) fn laid_lines(text: &str, max_width: f32, style: &TextStyle) -> Vec<TextLine> {
    metrics().laid_lines(text, None, max_width, style)
}

pub(crate) fn text_index_at(
    text: &str,
    spans: Option<&[Span]>,
    max_width: f32,
    style: &TextStyle,
    at: (f32, f32),
) -> Option<usize> {
    metrics().index_at(text, spans, max_width, style, at)
}

/// The drawn glyph extent `(ink_top, ink_height)` of `text` on the active surface. See [`TextMetrics::ink_bounds`].
pub fn measure_ink_bounds(text: &str, max_width: f32, style: &TextStyle) -> (f32, f32) {
    metrics().ink_bounds(text, max_width, style)
}

pub(crate) fn line_height(font_size: f32) -> f32 {
    metrics().line_height(font_size)
}

/// The size that sets `text`'s line to `width` on the active surface, no taller than `max_height`. See [`TextMetrics::fitted_size`].
pub fn fitted_font_size(
    text: &str,
    spans: Option<&[Span]>,
    width: f32,
    max_height: Option<f32>,
    style_at: &dyn Fn(f32) -> TextStyle,
) -> Option<f32> {
    metrics().fitted_size(text, spans, width, max_height, style_at)
}

/// The room a single-line box reserves for text of `font_size`.
///
/// Two different questions hide behind one number. On a surface that can put an edge anywhere, a line box is a design decision — how much air this design puts around a line — and [`SINGLE_LINE_LEADING`] is the amount. On a surface that quantises, it is not a decision at all: a terminal draws one glyph per cell whatever size the text claims, so a line occupies a cell and nothing may be added to it. A 32px title given the design's leading would reserve three rows to paint a single one, pushing everything below it off the grid.
///
/// So the grid decides which question is being asked, and a surface that can put an edge anywhere keeps the design's leading.
pub fn single_line_box(font_size: f32) -> f32 {
    if geometry_core::layout_grid().is_unit() {
        font_size * SINGLE_LINE_LEADING
    } else {
        line_height(font_size)
    }
}

/// What a box knows of the text in it, for a style written in it: lengths in the text the box holds, and the ink of the place the box sits in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxText {
    /// The size of the text the box holds, its own declaration included.
    pub em: f32,
    /// The height a single line of that text takes, as [`single_line_box`] reserves one.
    pub lh: f32,
    /// The ink the box is placed in: what it inherits, without what it declares itself, so a box that works its own text colour out from this one reads the same value it writes over.
    pub ink: Color,
}

impl BoxText {
    /// The text at `node`, subscribing the caller to every declaration it reads and to a face landing.
    pub fn of(node: layout_core::NodeId) -> Self {
        let _ = use_text_metrics_generation();
        let held = crate::inherit::inherited_text_style(node);
        let around = crate::inherit::text_around(node);
        Self::of_styles(&held, &around)
    }

    /// The text at the root of a surface, for a style built before its node exists.
    pub fn initial() -> Self {
        let root = crate::inherit::inherited_text_style_at(None);
        Self::of_styles(&root, &root)
    }

    fn of_styles(held: &TextStyle, around: &TextStyle) -> Self {
        Self {
            em: held.font_size,
            lh: single_line_box(held.font_size),
            ink: around.color.solid_color(),
        }
    }
}

/// The air a single-line box puts around its text where there is room to put any.
pub const SINGLE_LINE_LEADING: f32 = 1.4;

/// The height of one line **in this style**, which is the one the shaper will lay out: the multiple the style declares, or the face's own when it declares none.
///
/// The size alone is not the question, and asking it that way is how a caret ends up hanging below the letters it stands in. A tree that declares `line_height: 1.0` — a pixel face kept on its own grid — gets lines the height of the em, while the measurer's [`line_height`](TextMetrics::line_height) goes on answering with the face's natural leading, and every rectangle drawn to that answer is taller than the line it belongs to.
pub fn line_box(style: &TextStyle) -> f32 {
    match style.line_height.factor() {
        Some(factor) => style.font_size * factor,
        None => line_height(style.font_size),
    }
}

/// How many times the active surface's measurer may have started answering differently for the same text and style ([`TextMetrics::generation`]), counting a measurer opened over another one as a change. Read once rather than followed: see [`use_text_metrics_generation`].
pub fn text_metrics_generation() -> u64 {
    with_surface_metrics_ref(|slot| slot.measurer.as_ref().map_or(0, Measurer::generation))
}

fn followed_generation() -> Option<RwSignal<u64>> {
    with_surface_metrics_ref(|slot| slot.generation).filter(|generation| generation.is_alive())
}

/// [`text_metrics_generation`], read reactively: a memo or an effect that measures text itself reads this to run again when a face lands, since what it measured before may have been the fallback.
///
/// It moves on the frame after the face arrives, before that frame lays out, so whatever it re-runs is laid out in the same pass as the text the face re-measured.
pub fn use_text_metrics_generation() -> u64 {
    if let Some(generation) = followed_generation() {
        return generation.get();
    }
    // In the surface's own world, so the signal lives as long as the surface rather than as long as whichever scope asked first.
    let generation = in_surface_world(|| signal(text_metrics_generation()));
    with_surface_metrics(|slot| slot.generation = Some(generation));
    generation.get()
}

/// Moves [`use_text_metrics_generation`] up to the active surface's generation, and answers it.
pub(crate) fn follow_text_metrics_generation() -> u64 {
    let now = text_metrics_generation();
    if let Some(generation) = followed_generation()
        && generation.peek() != now
    {
        generation.set(now);
    }
    now
}

#[cfg(test)]
#[path = "text_metrics.test.rs"]
mod tests;
