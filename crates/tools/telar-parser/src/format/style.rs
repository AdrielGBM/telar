//! The `[style]` zone, re-emitted from the AST: one class per block, one blank line between them.

use crate::{RsxDocument, StyleClass};

use super::INDENT;

pub(super) enum StyleEntry<'a> {
    Class(&'a StyleClass),
}

pub(super) fn format_style_section(doc: &RsxDocument) -> String {
    let mut entries: Vec<(usize, StyleEntry)> = doc
        .style
        .classes
        .iter()
        .map(|class| (class.line, StyleEntry::Class(class)))
        .collect();
    entries.sort_by_key(|(line, _)| *line);

    let mut out = String::from("[style]");
    let mut prev_was_class = false;
    for (index, (_, entry)) in entries.iter().enumerate() {
        let is_class = matches!(entry, StyleEntry::Class(_));
        // A blank line sets classes apart from each other.
        if index > 0 && (is_class || prev_was_class) {
            out.push('\n');
        }
        out.push('\n');
        match entry {
            StyleEntry::Class(class) => {
                out.push_str(&format!("@{}", class.name));
                for prop in &class.props {
                    out.push('\n');
                    out.push_str(INDENT);
                    out.push_str(&format!("{}: {}", prop.key, prop.value));
                }
            }
        }
        prev_was_class = is_class;
    }
    out
}
