//! `textDocument/documentSymbol`: the `.rsx` outline / breadcrumbs.
//!
//! Surfaces the file's named, navigable symbols — `[style]` classes and `[preview]` sections — ordered by source line. The deep `[view]` element tree is intentionally omitted: it is mostly anonymous containers and would bury the useful entries.

use lsp_types::{DocumentSymbol, Position, Range, SymbolKind};
use telar_parser::RsxDocument;

/// The document's outline: its classes, its previews and the component it declares.
pub fn document_symbols(doc: &RsxDocument, source: &str) -> Vec<DocumentSymbol> {
    let mut entries: Vec<(usize, DocumentSymbol)> = Vec::new();

    for class in &doc.style.classes {
        entries.push((
            class.line,
            symbol(
                &format!("@{}", class.name),
                SymbolKind::CLASS,
                class.line,
                source,
            ),
        ));
    }
    for preview in &doc.previews {
        entries.push((
            preview.line,
            symbol(
                &format!("preview: {}", preview.name),
                SymbolKind::FUNCTION,
                preview.line,
                source,
            ),
        ));
    }

    entries.sort_by_key(|(line, _)| *line);
    entries.into_iter().map(|(_, sym)| sym).collect()
}

fn symbol(name: &str, kind: SymbolKind, line_1based: usize, source: &str) -> DocumentSymbol {
    let range = line_range(source, line_1based);
    #[allow(deprecated)] // `deprecated` is a required-but-deprecated field of DocumentSymbol.
    DocumentSymbol {
        name: name.to_string(),
        detail: None,
        kind,
        tags: None,
        deprecated: None,
        range,
        selection_range: range,
        children: None,
    }
}

/// The whole-line range (col 0 .. line length in UTF-16) of a 1-based line.
fn line_range(source: &str, line_1based: usize) -> Range {
    let line0 = line_1based.saturating_sub(1) as u32;
    let len = source
        .lines()
        .nth(line0 as usize)
        .map(|l| l.encode_utf16().count() as u32)
        .unwrap_or(0);
    Range {
        start: Position {
            line: line0,
            character: 0,
        },
        end: Position {
            line: line0,
            character: len,
        },
    }
}

#[cfg(test)]
#[path = "symbols_test.rs"]
mod tests;
