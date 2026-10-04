//! A frame written out as markup, for a page that has to show the app before its module has run.
//!
//! The same [`Frame`] the reconcile brings the live document in line with, written in the order the reconcile gives an element its attributes: what a browser parses out of this is what the reconcile would have built on a first frame, element for element and attribute for attribute. Nothing here asks a browser anything, so it runs wherever the app can be run without one.

use std::fmt::Write;
use std::rc::Rc;

use renderer_core::{Color, DrawCommand, ImageData, TextStyle};

use crate::document::{
    self, AUDIT_ATTRIBUTE, BoxNode, Content, DOCUMENT_SCROLL_OVERRIDES, HOST_ATTRIBUTE,
    ID_ATTRIBUTE, Node, PaintNode, RESET, RESET_ID, Surface,
};
use crate::runs::{RUN_ATTRIBUTE, RUN_BOX_ATTRIBUTE, Run};

/// One frame as a page writes it before the app runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prerendered {
    /// The attributes the element the app fills carries, in the order they are written: what marks it as the app's, and the surface's own background.
    pub host_attributes: Vec<(String, String)>,
    /// The element's content.
    pub markup: String,
}

impl Prerendered {
    /// The host's attributes as they are written inside its start tag, each with a leading space.
    pub fn host_attributes_html(&self) -> String {
        let mut out = String::new();
        for (name, value) in &self.host_attributes {
            attribute(&mut out, name, value);
        }
        out
    }
}

/// The stylesheet a document needs in its `<head>` for the boxes inside the host to look as they will once the app is running, as a `<style>` the running app recognises as its own and does not add again.
pub fn reset_stylesheet() -> String {
    format!("<style id=\"{RESET_ID}\">{RESET}</style>")
}

/// Writes the document `commands` describe, as a browser would first show it: the page at its top, with nothing asked of it yet.
///
/// A bitmap drawn inside a drawing is written with its address when it is linked from one, and without its pixels when the app made them, which only the running app can encode: the client that takes the page over fills in just the address. A masked line of text sits on a baseline estimated from its size rather than measured.
pub fn prerender(commands: &[DrawCommand], clear: Option<Color>) -> Prerendered {
    let mut surface = PageAtRest { scrolling: false };
    let frame = document::describe_frame(commands, clear, &mut surface, false);
    let mut style = String::from("position:relative;");
    if let Some(color) = frame.background {
        crate::paint::declare(&mut style, "background-color", &crate::paint::color(color));
        crate::paint::declare(&mut style, "color-scheme", crate::paint::scheme_of(color));
    }
    if frame.isolate_host {
        crate::paint::declare(&mut style, "isolation", "isolate");
    }
    let mut host_attributes = vec![(HOST_ATTRIBUTE.to_string(), String::new())];
    if frame.primary.is_some() {
        for (name, value) in DOCUMENT_SCROLL_OVERRIDES {
            crate::paint::declare(&mut style, name, value);
        }
        host_attributes.push((
            platform_core::primary_scroll::DOCUMENT_SCROLL_ATTRIBUTE.to_string(),
            String::new(),
        ));
    }
    host_attributes.push(("style".to_string(), style));
    let mut markup = String::new();
    for node in &frame.children {
        match node {
            Node::Box(node) => write_box(&mut markup, node),
            Node::Paint(node) => write_paint(&mut markup, node, None, false),
        }
    }
    Prerendered {
        host_attributes,
        markup,
    }
}

/// A page that has not scrolled and whose host stands at its origin, which is what a browser shows before anything has moved it.
struct PageAtRest {
    scrolling: bool,
}

impl Surface for PageAtRest {
    fn hold_document_scroll(&mut self, _id: u64) {
        self.scrolling = true;
    }

    fn fixed_origin(&mut self) -> Option<(f32, f32)> {
        self.scrolling.then_some((0.0, 0.0))
    }

    fn host_origin(&mut self) -> (f32, f32) {
        (0.0, 0.0)
    }

    fn image_href(&mut self, data: &ImageData) -> Option<Rc<str>> {
        data.linked_source()
            .map(|linked| platform_core::asset_url(&linked.url).into())
    }

    /// Half the leading the line box adds, then an ascent of four fifths of the size, which is what most faces come to.
    fn baseline(&mut self, style: &TextStyle) -> f32 {
        let size = style.font_size;
        let line = match style.line_height {
            renderer_core::LineHeight::Times(factor) => size * factor,
            renderer_core::LineHeight::Natural => size * 1.2,
        };
        (line - size) / 2.0 + size * 0.8
    }
}

fn write_box(out: &mut String, node: &BoxNode) {
    let _ = write!(out, "<{}", node.tag);
    attribute(out, ID_ATTRIBUTE, &node.id.to_string());
    for (name, value) in node.described.attributes(node.id) {
        if let Some(value) = value {
            attribute(out, name, &value);
        }
    }
    if let Some((picture, width)) = &node.picture {
        for (name, value) in document::picture_attributes(picture, *width) {
            if let Some(value) = value {
                attribute(out, name, &value);
            }
        }
    }
    if let Some(rect) = node.audit {
        let rect = format!("{} {} {} {}", rect.x, rect.y, rect.width, rect.height);
        attribute(out, AUDIT_ATTRIBUTE, &rect);
    }
    if !node.style.is_empty() {
        attribute(out, "style", &node.style);
    }
    out.push('>');
    if node.tag == "img" {
        return;
    }
    match &node.content {
        Content::Children { boxes, pieces } => {
            for child in boxes {
                write_box(out, child);
            }
            for piece in pieces {
                write_paint(out, piece, Some(node.id), true);
            }
        }
        Content::Text { text, runs } => match runs {
            Some(runs) => write_runs(out, runs, node.id),
            None => text_into(out, text),
        },
        Content::Drawing(markup) => out.push_str(markup),
    }
    let _ = write!(out, "</{}>", node.tag);
}

/// Paint that is not a box. Inside a box it is out of the accessibility tree, where a role with a content model would count it as a child; at the host's own level it is a panel the boxes stand on.
fn write_paint(out: &mut String, node: &PaintNode, owner: Option<u64>, presentation: bool) {
    out.push_str("<div");
    if presentation {
        attribute(out, "role", "presentation");
    }
    if !node.style.is_empty() {
        attribute(out, "style", &node.style);
    }
    out.push('>');
    match (&node.runs, owner) {
        (Some(runs), Some(owner)) => write_runs(out, runs, owner),
        _ => text_into(out, &node.text),
    }
    out.push_str("</div>");
}

fn write_runs(out: &mut String, runs: &[Run], owner: u64) {
    for run in runs {
        match (&run.link, run.css.is_empty()) {
            (None, true) => text_into(out, &run.text),
            (link, _) => {
                let tag = if link.is_some() { "a" } else { "span" };
                let _ = write!(out, "<{tag}");
                if !run.css.is_empty() {
                    attribute(out, "style", &run.css);
                }
                if let Some(link) = link {
                    attribute(out, "href", &link.href);
                    if link.opens_beside {
                        attribute(out, "target", "_blank");
                    }
                    if link.external {
                        attribute(out, "rel", "noopener");
                    }
                    attribute(out, RUN_BOX_ATTRIBUTE, &owner.to_string());
                    attribute(out, RUN_ATTRIBUTE, &link.run.to_string());
                }
                out.push('>');
                text_into(out, &run.text);
                let _ = write!(out, "</{tag}>");
            }
        }
    }
}

/// An attribute with its value escaped, written the way a browser writes one back.
fn attribute(out: &mut String, name: &str, value: &str) {
    out.push(' ');
    out.push_str(name);
    out.push_str("=\"");
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn text_into(out: &mut String, text: &str) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\u{a0}' => out.push_str("&nbsp;"),
            c => out.push(c),
        }
    }
}

#[cfg(test)]
#[path = "html_test.rs"]
mod tests;
