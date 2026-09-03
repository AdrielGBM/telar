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
        // What browsers use for `line-height: normal` on most faces. Approximate by construction: the exact
        // value is the font's own, and a canvas will not report it.
        renderer_core::LineHeight::Natural => style.font_size * 1.2,
    }
}

/// Greedy word wrap, which is what a browser does for text with no hyphenation or `text-wrap: pretty`.
fn wrap(text: &str, max_width: f32, style: &TextStyle) -> (f32, usize) {
    let mut widest: f32 = 0.0;
    let mut lines = 0usize;

    for hard in text.split('\n') {
        lines += 1;
        if style.text_wrap == TextWrap::NoWrap || !max_width.is_finite() || max_width >= 1.0e5 {
            widest = widest.max(width_of(hard, style));
            continue;
        }
        let mut line_start = 0usize;
        let mut last_break: Option<usize> = None;
        for (offset, c) in hard.char_indices() {
            if c.is_whitespace() {
                last_break = Some(offset);
            }
            let candidate = &hard[line_start..offset + c.len_utf8()];
            if width_of(candidate, style) <= max_width {
                continue;
            }
            let cut = match last_break {
                Some(at) if at > line_start => at,
                // A word wider than the column breaks inside itself, as `overflow-wrap` does.
                _ => offset.max(line_start + c.len_utf8()),
            };
            widest = widest.max(width_of(&hard[line_start..cut], style));
            line_start = hard[cut..]
                .find(|c: char| !c.is_whitespace())
                .map(|skip| cut + skip)
                .unwrap_or(hard.len());
            last_break = None;
            lines += 1;
        }
        widest = widest.max(width_of(&hard[line_start..], style));
    }

    if let Some(max) = style.clamp.max_lines() {
        lines = lines.min(max);
    }
    (widest, lines.max(1))
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

    fn line_height(&self, font_size: f32) -> f32 {
        font_size * 1.2
    }
}
