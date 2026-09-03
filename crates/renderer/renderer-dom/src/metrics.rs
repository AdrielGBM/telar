//! How wide a string is, asked of the browser that will draw it.
//!
//! Taffy still runs on this target — its rects are what hit-testing, scrolling and anchored overlays read,
//! and what a parity test compares the browser's own layout against. For those numbers to mean anything,
//! the measurer has to agree with whatever will actually render the text, so it asks a 2D canvas rather
//! than shaping glyphs from a font file the page would then not use.

use std::cell::RefCell;

use renderer_core::{Span, TextMetrics, TextStyle, TextWrap};
use wasm_bindgen::JsCast;

// The context lives here rather than in the measurer so the measurer stays a unit struct and satisfies the
// `Send + Sync` the runtime's slot is declared with — which on a target with one thread costs nothing and
// means nothing, but has to be true for it to be installed at all.
thread_local! {
    static CONTEXT: RefCell<Option<web_sys::CanvasRenderingContext2d>> = const { RefCell::new(None) };
    /// A face's line box, which is a property of the face and the size and of nothing a paragraph does.
    static LINE_BOXES: RefCell<rustc_hash::FxHashMap<String, f32>> =
        RefCell::new(rustc_hash::FxHashMap::default());
}

fn with_context<R>(f: impl FnOnce(&web_sys::CanvasRenderingContext2d) -> R) -> Option<R> {
    CONTEXT.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_none() {
            *slot = make_context();
        }
        slot.as_ref().map(f)
    })
}

fn make_context() -> Option<web_sys::CanvasRenderingContext2d> {
    // Never added to the document: a canvas measures text just as well detached, and one on the page would
    // be a stray element in every app that uses this backend.
    let document = web_sys::window()?.document()?;
    document
        .create_element("canvas")
        .ok()?
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .ok()?
        .get_context("2d")
        .ok()??
        .dyn_into::<web_sys::CanvasRenderingContext2d>()
        .ok()
}

/// The `font` shorthand a 2D context takes, from a Telar text style.
fn font_of(style: &TextStyle) -> String {
    let family = match &style.font_family {
        renderer_core::FontFamily::Named(name) => format!("\"{name}\",sans-serif"),
        renderer_core::FontFamily::SansSerif => "sans-serif".to_string(),
    };
    let slant = if style.font_style == renderer_core::FontStyle::Normal {
        ""
    } else {
        "italic "
    };
    format!(
        "{slant}{} {}px {family}",
        style.font_weight, style.font_size
    )
}

fn width_of(text: &str, style: &TextStyle) -> f32 {
    if text.is_empty() {
        return 0.0;
    }
    with_context(|ctx| {
        ctx.set_font(&font_of(style));
        ctx.measure_text(text)
            .map(|m| m.width() as f32)
            .unwrap_or(0.0)
    })
    .unwrap_or(0.0)
        + style.letter_spacing * text.chars().count().saturating_sub(1) as f32
}

/// A line box, which CSS derives from the font size unless something says otherwise.
fn line_height(style: &TextStyle) -> f32 {
    match style.line_height {
        renderer_core::LineHeight::Times(factor) => style.font_size * factor,
        renderer_core::LineHeight::Natural => natural(style),
    }
}

/// What `line-height: normal` comes to for this face at this size — the font's own ascent and descent,
/// which is what the browser will lay the line out with.
///
/// Guessing it as a multiple of the size is close and wrong, and wrong by a pixel a row accumulates: a list
/// of twenty items ended forty pixels below where hit-testing believed it was.
fn natural(style: &TextStyle) -> f32 {
    let font = font_of(style);
    if let Some(known) = LINE_BOXES.with(|cache| cache.borrow().get(&font).copied()) {
        return known;
    }
    let measured = laid_out_line(&font)
        // A page that will not lay out a probe still has to be measured for, and this is what most faces
        // come to.
        .unwrap_or(style.font_size * 1.2);
    LINE_BOXES.with(|cache| cache.borrow_mut().insert(font, measured));
    measured
}

/// The paragraph as the browser would break it, in the style it will be drawn in.
fn wrap(text: &str, max_width: f32, style: &TextStyle) -> (f32, usize) {
    // Text that must stay on one line, and a column nothing could overflow, are the same instruction to a
    // wrap: never break.
    let column =
        if style.text_wrap == TextWrap::NoWrap || !max_width.is_finite() || max_width >= 1.0e5 {
            f32::INFINITY
        } else {
            max_width
        };
    let (widest, mut lines) = crate::wrap::greedy(text, column, |run| width_of(run, style));
    if let Some(max) = style.clamp.max_lines() {
        lines = lines.min(max);
    }
    (widest, lines.max(1))
}

/// How tall one line comes out when the browser lays it out, asked of the browser.
///
/// A canvas reports the *font's* box, and `line-height: normal` is not quite that — the difference is under
/// a pixel and a column of twenty rows is twenty of them. So the question is put to the thing that will
/// answer it later anyway: a box with this font and one line in it, measured and thrown away. It costs a
/// layout, which is why the answer is kept: a face at a size has one line height and nothing a paragraph
/// does changes it.
fn laid_out_line(font: &str) -> Option<f32> {
    let document = web_sys::window()?.document()?;
    let body = document.body()?;
    let probe = document
        .create_element("div")
        .ok()?
        .dyn_into::<web_sys::HtmlElement>()
        .ok()?;
    probe
        .set_attribute(
            "style",
            &format!(
                "position:absolute;top:-9999px;left:-9999px;visibility:hidden;white-space:pre;font:{font}"
            ),
        )
        .ok()?;
    probe.set_text_content(Some("Hg"));
    body.append_child(probe.as_ref()).ok()?;
    let height = probe.get_bounding_client_rect().height() as f32;
    probe.remove();
    (height > 0.0).then_some(height)
}

/// Measures with the same engine that will draw.
#[derive(Clone, Copy, Debug, Default)]
pub struct CanvasTextMetrics;

impl TextMetrics for CanvasTextMetrics {
    /// Spans change colour and weight within a paragraph. Measuring each separately and summing would be
    /// more faithful; measuring the whole run in the paragraph's own style is what the layout above asks
    /// for, and the difference only shows where a span changes the weight of a long line.
    fn measure(
        &self,
        text: &str,
        _spans: Option<&[Span]>,
        max_width: f32,
        style: &TextStyle,
    ) -> (f32, f32) {
        let (width, lines) = wrap(text, max_width, style);
        (width, lines as f32 * line_height(style))
    }

    fn ink_bounds(&self, text: &str, max_width: f32, style: &TextStyle) -> (f32, f32) {
        self.measure(text, None, max_width, style)
    }

    /// Asked with a size and nothing else, which is the default face at that size.
    fn line_height(&self, font_size: f32) -> f32 {
        natural(&TextStyle::new(font_size, renderer_core::Color::BLACK))
    }
}
