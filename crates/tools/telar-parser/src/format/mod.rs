//! Canonical formatter for `.rsx` documents.
//!
//! A `.rsx` file is reformatted section by section:
//! - `[logic]` runs through `rustfmt`, so imports get reordered and long statements wrap exactly like a `.rs` file. The zone is statement-level Rust (`let` bindings live at its top level), which is not a valid item on its own, so it is wrapped in a synthetic `fn { ... }` before formatting and unwrapped afterwards.
//! - `[style]` and `[view]` are re-emitted from the parsed AST in a canonical shape: 4-space indentation, single-space token separators, and one blank line between style classes.
//!
//! Formatting is whole-document: the parsed AST is re-serialized and the backend returns it as a single replacement edit, so it never has to map edits back through the section line offsets.

mod logic;
mod style;
mod view;

use crate::{Section, header_section, parse};

use logic::format_logic_section;
use style::format_style_section;
use view::{format_preview_section, format_view_section};

const INDENT: &str = "    ";

/// Formats a whole `.rsx` document. Returns `None` when the source does not parse (an invalid document is left untouched, as every formatter does).
pub fn format_document(source: &str) -> Option<String> {
    let doc = parse(source).ok()?;
    let present = present_sections(source);

    let mut sections: Vec<String> = Vec::new();

    if present.contains(&Section::Logic) || !doc.logic.source.trim().is_empty() {
        sections.push(format_logic_section(&doc.logic.source));
    }
    if present.contains(&Section::Style) || !doc.style.classes.is_empty() {
        sections.push(format_style_section(&doc));
    }
    if present.contains(&Section::View) || !doc.view.nodes.is_empty() {
        sections.push(format_view_section(&doc.view.nodes));
    }
    for preview in &doc.previews {
        sections.push(format_preview_section(preview));
    }

    if sections.is_empty() {
        return Some(String::new());
    }

    let mut out = sections.join("\n\n");
    out.push('\n');
    Some(out)
}

/// Returns the section headers present in `source`, in order of first appearance, so empty-but-declared sections are preserved by the formatter.
fn present_sections(source: &str) -> Vec<Section> {
    let mut present = Vec::new();
    for line in source.lines() {
        if let Some(section) = header_section(line.trim())
            && !present.contains(&section)
        {
            present.push(section);
        }
    }
    present
}

#[cfg(test)]
#[path = "format_test.rs"]
mod tests;
