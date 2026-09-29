//! How wide a string is, asked of the browser that will draw it.
//!
//! Taffy still runs on this target — its rects are what hit-testing, scrolling and anchored overlays read, and what a parity test compares the browser's own layout against. For those numbers to mean anything, the measurer has to agree with whatever will actually render the text, so it asks a 2D canvas rather than shaping glyphs from a font file the page would then not use.

use std::cell::RefCell;

use renderer_core::{Span, TextMetrics, TextStyle, TextWrap};
use wasm_bindgen::JsCast;

// Here rather than in the measurer, so the measurer stays a unit struct and satisfies the `Send + Sync` its runtime slot is declared with — which on a single-threaded target means nothing, but has to be true for it to be installed at all.
thread_local! {
    static CONTEXT: RefCell<Option<web_sys::CanvasRenderingContext2d>> = const { RefCell::new(None) };
    /// A face's line box, which is a property of the face and the size and of nothing a paragraph does.
    static LINE_BOXES: RefCell<rustc_hash::FxHashMap<String, f32>> =
        RefCell::new(rustc_hash::FxHashMap::default());
    /// Widths measured with a laid-out probe, by the style's CSS and the text: see [`needs_probe`].
    static PROBED: RefCell<rustc_hash::FxHashMap<(String, String), f32>> =
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
    // Never added to the document: a canvas measures text just as well detached, and one on the page would be a stray element in every app that uses this backend.
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
    // The canvas `font` shorthand takes the same family-list grammar CSS does, so the DOM's own serializer is the measurer's too — the two have to agree on what a generic resolves to, or the number this reports is for a face the page will not draw.
    let family = crate::paint::font_family_list(&style.font_family)
        .unwrap_or_else(|| "sans-serif".to_string());
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

/// Whether measuring `style` needs a laid-out element rather than a canvas: a canvas `font` carries family, size, weight and slant, and nothing sets the axes or the OpenType features on it. Not even `wght`: standing it in for the weight would have the browser embolden a face whose `@font-face` names one weight, which the page, setting the axis, never does.
fn needs_probe(style: &TextStyle) -> bool {
    !style.font_features.is_empty() || !style.font_variations.is_empty()
}

/// Everything about `style` a laid-out probe needs beyond the `font` shorthand.
fn probe_settings(style: &TextStyle) -> String {
    format!(
        "font-variation-settings:{};font-feature-settings:{}",
        style.font_variations.to_css(),
        style.font_features.to_css()
    )
}

/// How wide `text` is in `style`, tracking included. Tracking follows every character, the last one too, as CSS and the GPU shaper both apply it: a line tracked tight ends that much short of its last glyph's advance.
fn width_of(text: &str, style: &TextStyle) -> f32 {
    if text.is_empty() {
        return 0.0;
    }
    if needs_probe(style) {
        return probed_width(text, style) + style.letter_spacing * text.chars().count() as f32;
    }
    with_context(|ctx| {
        ctx.set_font(&font_of(style));
        ctx.measure_text(text)
            .map(|m| m.width() as f32)
            .unwrap_or(0.0)
    })
    .unwrap_or(0.0)
        + style.letter_spacing * text.chars().count() as f32
}

/// How wide `text` comes out when the browser lays it out in `style`, asked of a hidden element and kept: the one way to measure axes and features a canvas cannot be told about.
fn probed_width(text: &str, style: &TextStyle) -> f32 {
    let key = (
        format!("{};{}", font_of(style), probe_settings(style)),
        text.to_owned(),
    );
    if let Some(known) = PROBED.with(|cache| cache.borrow().get(&key).copied()) {
        return known;
    }
    let measured = laid_out(&font_of(style), &probe_settings(style), text)
        .map(|(width, _)| width)
        .unwrap_or(0.0);
    PROBED.with(|cache| {
        let mut cache = cache.borrow_mut();
        // An axis animated through a range measures every word at every step it passes; kept whole, that is a cache that only grows.
        if cache.len() >= PROBED_CAPACITY {
            cache.clear();
        }
        cache.insert(key, measured);
    });
    measured
}

const PROBED_CAPACITY: usize = 4096;

/// A line box, which CSS derives from the font size unless something says otherwise.
fn line_height(style: &TextStyle) -> f32 {
    match style.line_height {
        renderer_core::LineHeight::Times(factor) => style.font_size * factor,
        renderer_core::LineHeight::Natural => natural(style),
    }
}

/// What `line-height: normal` comes to for this face at this size — the font's own ascent and descent, which is what the browser will lay the line out with.
///
/// Guessing it as a multiple of the size is close and wrong, and wrong by a pixel a row accumulates: a list of twenty items ended forty pixels below where hit-testing believed it was.
fn natural(style: &TextStyle) -> f32 {
    let settings = if needs_probe(style) {
        probe_settings(style)
    } else {
        String::new()
    };
    let font = font_of(style);
    let key = format!("{font};{settings}");
    if let Some(known) = LINE_BOXES.with(|cache| cache.borrow().get(&key).copied()) {
        return known;
    }
    let measured = laid_out(&font, &settings, "Hg")
        .map(|(_, height)| height)
        .filter(|height| *height > 0.0)
        // A page that will not lay out a probe still has to be measured for, and this is what most faces come to.
        .unwrap_or(style.font_size * 1.2);
    LINE_BOXES.with(|cache| cache.borrow_mut().insert(key, measured));
    measured
}

/// How far below the top of its line box a line in `style` sits on its baseline: half the leading the line box adds, then the face's ascent, as a document lays a line out.
pub(crate) fn baseline(style: &TextStyle) -> f32 {
    let (ascent, descent) = with_context(|ctx| {
        ctx.set_font(&font_of(style));
        ctx.measure_text("Hg")
            .map(|m| {
                (
                    m.font_bounding_box_ascent() as f32,
                    m.font_bounding_box_descent() as f32,
                )
            })
            .unwrap_or((style.font_size * 0.8, style.font_size * 0.2))
    })
    .unwrap_or((style.font_size * 0.8, style.font_size * 0.2));
    (line_height(style) - (ascent + descent)) / 2.0 + ascent
}

/// The paragraph as the browser would break it, in the style it will be drawn in.
fn wrap(text: &str, max_width: f32, style: &TextStyle) -> (f32, usize) {
    // Text that must stay on one line, and a column nothing could overflow, are the same instruction to a wrap.
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

/// How wide and tall `text` comes out on one line when the browser lays it out in `font` and `settings`, asked of the browser.
///
/// Measures a line box too: a canvas reports the *font's* box, and `line-height: normal` is not quite that — the difference is under a pixel and a column of twenty rows is twenty of them. So the question is put to the thing that will answer it later anyway: a box with this font and one line in it, measured and thrown away. It costs a layout, which is why the answer is kept: a face at a size has one line height and nothing a paragraph does changes it.
fn laid_out(font: &str, settings: &str, text: &str) -> Option<(f32, f32)> {
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
                "position:absolute;top:-9999px;left:-9999px;visibility:hidden;white-space:pre;font:{font};{settings}"
            ),
        )
        .ok()?;
    probe.set_text_content(Some(text));
    body.append_child(probe.as_ref()).ok()?;
    let rect = probe.get_bounding_client_rect();
    probe.remove();
    Some((rect.width() as f32, rect.height() as f32))
}

/// Measures with the same engine that will draw.
#[derive(Clone, Copy, Debug, Default)]
pub struct CanvasTextMetrics;

impl TextMetrics for CanvasTextMetrics {
    /// Spans change colour and weight within a paragraph. Measuring each separately and summing would be more faithful; measuring the whole run in the paragraph's own style is what the layout above asks for, and the difference only shows where a span changes the weight of a long line.
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

    fn min_content(&self, text: &str, spans: Option<&[Span]>, style: &TextStyle) -> (f32, f32) {
        let widest = crate::wrap::widest_word(text, |word| width_of(word, style));
        self.measure(text, spans, widest, style)
    }

    fn ink_bounds(&self, text: &str, max_width: f32, style: &TextStyle) -> (f32, f32) {
        self.measure(text, None, max_width, style)
    }

    /// Asked with a size and nothing else, which is the default face at that size.
    fn line_height(&self, font_size: f32) -> f32 {
        natural(&TextStyle::new(font_size, renderer_core::Color::BLACK))
    }
}

/// Measures every text again, once, each time the page finishes loading fonts.
///
/// A face the page declares with `@font-face` arrives after the first layout — nothing waits for it, which is what `font-display: swap` promises — so text was measured in whatever the browser fell back to. The browser re-flows its own boxes when the face lands; Telar's layout, which hit-testing and scrolling read, has to be told. `loadingdone` fires once per batch of faces that finished together, so one batch is one relayout however many faces it held.
pub fn remeasure_on_font_load() {
    thread_local! {
        static LISTENING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    if LISTENING.with(|listening| listening.replace(true)) {
        return;
    }
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };
    let on_loaded =
        wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::Event)>::new(|_: web_sys::Event| {
            fonts_loaded()
        });
    let _ = document
        .fonts()
        .add_event_listener_with_callback("loadingdone", on_loaded.as_ref().unchecked_ref());
    // Listening for the life of the page, which is the life of the app.
    on_loaded.forget();
}

fn fonts_loaded() {
    LINE_BOXES.with(|cache| cache.borrow_mut().clear());
    PROBED.with(|cache| cache.borrow_mut().clear());
    renderer_core::invalidate_text_metrics();
    if let Some(wake) = platform_core::loop_waker() {
        wake();
    }
}
