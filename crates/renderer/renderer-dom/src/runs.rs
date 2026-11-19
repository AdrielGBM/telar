//! A paragraph with spans as a document writes it: one inline element per span, an `<a href>` for a span that links, a `<ruby>` for a span with a reading, and plain text between them.

use std::fmt::Write;

use renderer_core::{Destination, Span, TextStyle};

/// The attribute naming the text box an in-paragraph link belongs to, for the click a document takes back.
pub(crate) const RUN_BOX_ATTRIBUTE: &str = "data-telar-run-box";
/// The attribute naming which of the text's spans an in-paragraph link is.
pub(crate) const RUN_ATTRIBUTE: &str = "data-telar-run";

/// The style a `<ruby>` holds its base with: on one line, as every other target keeps a base.
pub(crate) const RUBY_STYLE: &str = "white-space:nowrap;";

/// One stretch of a paragraph: its text, what its span declares over the paragraph, where it links, and the reading set above it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Run {
    pub text: String,
    pub css: String,
    pub link: Option<RunLink>,
    pub ruby: Option<RunRuby>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RunLink {
    pub href: String,
    pub external: bool,
    pub opens_beside: bool,
    pub run: u32,
}

/// A reading over a run, as its `<rt>` writes it: the reading, and the size and tightening the run's fit gave it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RunRuby {
    pub reading: String,
    pub css: String,
}

impl Run {
    fn plain(text: &str) -> Self {
        Self {
            text: text.to_owned(),
            css: String::new(),
            link: None,
            ruby: None,
        }
    }
}

/// `text` cut at its spans, in order, with the gaps between them as plain runs. A link's `run` is its span's index in `spans`. A size in pixels is written in `rem` of `root_font_size`, as the paragraph's is, and a reading is fitted to its base in the run's style over `style`, the paragraph's, as `width_of` measures the two (see [`renderer_core::RubyStyle::fit`]).
pub(crate) fn runs_of(
    text: &str,
    spans: &[Span],
    style: &TextStyle,
    root_font_size: f32,
    width_of: &mut dyn FnMut(&str, &TextStyle) -> f32,
) -> Vec<Run> {
    renderer_core::span_segments(text, spans)
        .map(|segment| {
            let piece = &text[segment.range];
            let Some((index, span)) = segment.span else {
                return Run::plain(piece);
            };
            let mut css = String::new();
            crate::paint::span_style(&span.over, root_font_size, &mut css);
            let link = span.link.as_ref().map(|destination| RunLink {
                href: platform_core::address_of(destination),
                external: matches!(destination, Destination::External(_)),
                opens_beside: matches!(destination, Destination::External(uri) if uri.is_web()),
                run: index as u32,
            });
            let ruby = span.ruby.as_deref().map(|reading| {
                let run_style = span.over.over(style, geometry_core::Size::ZERO);
                let fit = fit_reading(piece, reading, &run_style, width_of);
                let mut css = String::new();
                crate::paint::reading_style(&fit, run_style.font_size, &mut css);
                RunRuby {
                    reading: reading.to_owned(),
                    css,
                }
            });
            Run {
                text: piece.to_owned(),
                css,
                link,
                ruby,
            }
        })
        .collect()
}

/// How `reading` is set over `base`, a run in `run_style`, with each measured by `width_of`.
pub(crate) fn fit_reading(
    base: &str,
    reading: &str,
    run_style: &TextStyle,
    width_of: &mut dyn FnMut(&str, &TextStyle) -> f32,
) -> renderer_core::RubyFit {
    let nominal = run_style.ruby.nominal(run_style.font_size);
    let reading_style = TextStyle {
        letter_spacing: 0.0,
        ..run_style.clone().with_font_size(nominal)
    };
    let letters = unicode_segmentation::UnicodeSegmentation::graphemes(reading, true).count();
    run_style.ruby.fit(
        run_style.font_size,
        width_of(base, run_style),
        width_of(reading, &reading_style),
        letters,
    )
}

/// A string that changes whenever what the runs write does, for telling a frame that restyled a run from one that did not.
pub(crate) fn signature(runs: &[Run]) -> String {
    let mut out = String::from("\u{1}");
    for run in runs {
        let _ = write!(out, "{}\u{2}{}\u{2}", run.text, run.css);
        if let Some(link) = &run.link {
            let _ = write!(out, "{}\u{2}{}", link.href, link.run);
        }
        if let Some(ruby) = &run.ruby {
            let _ = write!(out, "\u{4}{}\u{2}{}", ruby.reading, ruby.css);
        }
        out.push('\u{3}');
    }
    out
}

/// Replaces `node`'s children with `runs`, the text of box `owner`.
#[cfg(target_arch = "wasm32")]
pub(crate) fn write(
    document: &web_sys::Document,
    node: &web_sys::Element,
    runs: &[Run],
    owner: crate::box_key::BoxKey,
) {
    node.set_text_content(None);
    for run in runs {
        let content: web_sys::Node = match &run.ruby {
            None => document.create_text_node(&run.text).into(),
            Some(ruby) => match ruby_element(document, &run.text, ruby) {
                Some(element) => element.into(),
                None => continue,
            },
        };
        let child: web_sys::Node = match (&run.link, run.css.is_empty()) {
            (None, true) => content,
            (link, _) => {
                let tag = if link.is_some() { "a" } else { "span" };
                let Ok(element) = document.create_element(tag) else {
                    continue;
                };
                if !run.css.is_empty() {
                    let _ = element.set_attribute("style", &run.css);
                }
                if let Some(link) = link {
                    let _ = element.set_attribute("href", &link.href);
                    if link.opens_beside {
                        let _ = element.set_attribute("target", "_blank");
                    }
                    if link.external {
                        let _ = element.set_attribute("rel", "noopener");
                    }
                    let _ = element.set_attribute(RUN_BOX_ATTRIBUTE, &owner.to_string());
                    let _ = element.set_attribute(RUN_ATTRIBUTE, &link.run.to_string());
                }
                let _ = element.append_child(&content);
                element.into()
            }
        };
        let _ = node.append_child(&child);
    }
}

/// `<ruby>base<rp>(</rp><rt>reading</rt><rp>)</rp></ruby>`: the parentheses are what a browser that sets no ruby shows, and what a screen reader reading the text aloud says around the reading.
#[cfg(target_arch = "wasm32")]
fn ruby_element(
    document: &web_sys::Document,
    base: &str,
    ruby: &RunRuby,
) -> Option<web_sys::Element> {
    let element = document.create_element("ruby").ok()?;
    let _ = element.set_attribute("style", RUBY_STYLE);
    let _ = element.append_child(&document.create_text_node(base));
    let parenthesis = |mark: &str| -> Option<web_sys::Node> {
        let rp = document.create_element("rp").ok()?;
        rp.set_text_content(Some(mark));
        Some(rp.into())
    };
    let rt = document.create_element("rt").ok()?;
    let _ = rt.set_attribute("style", &ruby.css);
    rt.set_text_content(Some(&ruby.reading));
    let _ = element.append_child(&parenthesis("(")?);
    let _ = element.append_child(&rt);
    let _ = element.append_child(&parenthesis(")")?);
    Some(element)
}

/// The markup of `ruby_element`, for a document written ahead of time.
pub(crate) fn ruby_markup(out: &mut String, base: &str, ruby: &RunRuby) {
    out.push_str("<ruby");
    crate::markup::attribute(out, "style", RUBY_STYLE);
    out.push('>');
    out.push_str(&crate::markup::escape_html(base));
    out.push_str("<rp>(</rp><rt");
    crate::markup::attribute(out, "style", &ruby.css);
    out.push('>');
    out.push_str(&crate::markup::escape_html(&ruby.reading));
    out.push_str("</rt><rp>)</rp></ruby>");
}

#[cfg(test)]
#[path = "runs.test.rs"]
mod tests;
