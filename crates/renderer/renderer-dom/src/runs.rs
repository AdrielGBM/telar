//! A paragraph with spans as a document writes it: one inline element per span, an `<a href>` for a span that links, and plain text between them.

use std::fmt::Write;

use renderer_core::{Destination, Span};

/// The attribute naming the text box an in-paragraph link belongs to, for the click a document takes back.
pub(crate) const RUN_BOX_ATTRIBUTE: &str = "data-telar-run-box";
/// The attribute naming which of the text's spans an in-paragraph link is.
pub(crate) const RUN_ATTRIBUTE: &str = "data-telar-run";

/// One stretch of a paragraph: its text, what its span declares over the paragraph, and where it links.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Run {
    pub text: String,
    pub css: String,
    pub link: Option<RunLink>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RunLink {
    pub href: String,
    pub external: bool,
    pub opens_beside: bool,
    pub run: u32,
}

impl Run {
    fn plain(text: &str) -> Self {
        Self {
            text: text.to_owned(),
            css: String::new(),
            link: None,
        }
    }
}

/// `text` cut at its spans, in order, with the gaps between them as plain runs. A span out of order, past the end or off a character boundary is skipped rather than trusted, as the shaper does.
pub(crate) fn runs_of(text: &str, spans: &[Span]) -> Vec<Run> {
    let mut runs = Vec::with_capacity(spans.len() * 2 + 1);
    let mut at = 0usize;
    for (index, span) in spans.iter().enumerate() {
        let start = (span.range.start as usize).clamp(at, text.len());
        let end = (span.range.end as usize).clamp(start, text.len());
        if !text.is_char_boundary(start) || !text.is_char_boundary(end) || start == end {
            continue;
        }
        if start > at {
            runs.push(Run::plain(&text[at..start]));
        }
        let mut css = String::new();
        crate::paint::span_style(&span.over, &mut css);
        let link = span.link.as_ref().map(|destination| RunLink {
            href: platform_core::address_of(destination),
            external: matches!(destination, Destination::External(_)),
            opens_beside: matches!(destination, Destination::External(uri) if uri.is_web()),
            run: index as u32,
        });
        runs.push(Run {
            text: text[start..end].to_owned(),
            css,
            link,
        });
        at = end;
    }
    if at < text.len() {
        runs.push(Run::plain(&text[at..]));
    }
    runs
}

/// A string that changes whenever what the runs write does, for telling a frame that restyled a run from one that did not.
pub(crate) fn signature(runs: &[Run]) -> String {
    let mut out = String::from("\u{1}");
    for run in runs {
        let _ = write!(out, "{}\u{2}{}\u{2}", run.text, run.css);
        if let Some(link) = &run.link {
            let _ = write!(out, "{}\u{2}{}", link.href, link.run);
        }
        out.push('\u{3}');
    }
    out
}

/// Replaces `node`'s children with `runs`, the text of `box_id`.
#[cfg(target_arch = "wasm32")]
pub(crate) fn write(
    document: &web_sys::Document,
    node: &web_sys::Element,
    runs: &[Run],
    box_id: u64,
) {
    node.set_text_content(None);
    for run in runs {
        let child: web_sys::Node = match (&run.link, run.css.is_empty()) {
            (None, true) => document.create_text_node(&run.text).into(),
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
                    let _ = element.set_attribute(RUN_BOX_ATTRIBUTE, &box_id.to_string());
                    let _ = element.set_attribute(RUN_ATTRIBUTE, &link.run.to_string());
                }
                element.set_text_content(Some(&run.text));
                element.into()
            }
        };
        let _ = node.append_child(&child);
    }
}

#[cfg(test)]
#[path = "runs_test.rs"]
mod tests;
