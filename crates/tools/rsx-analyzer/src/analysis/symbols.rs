//! `textDocument/documentSymbol`: the `.rsx` outline / breadcrumbs.
//!
//! Surfaces the file's named, navigable symbols — `[style]` constants and classes, and `[preview]` sections — ordered by source line. The deep `[view]` element tree is intentionally omitted: it is mostly anonymous containers and would bury the useful entries.

use lsp_types::{DocumentSymbol, Position, Range, SymbolKind};
use rsx_parser::RsxDocument;

pub fn document_symbols(doc: &RsxDocument, source: &str) -> Vec<DocumentSymbol> {
    let mut entries: Vec<(usize, DocumentSymbol)> = Vec::new();

    for constant in &doc.style.constants {
        entries.push((
            constant.line,
            symbol(&constant.name, SymbolKind::CONSTANT, constant.line, source),
        ));
    }
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
mod tests {
    use super::*;
    use rsx_parser::parse;

    #[test]
    fn outlines_constants_classes_and_previews_in_order() {
        let src = "[style]\nprimary: #4361ee\n\n@card\n    width: 240\n\n[view]\ncol @card\n\n[preview \"Default\"]\ncard\n";
        let doc = parse(src).unwrap();
        let syms = document_symbols(&doc, src);
        let names: Vec<&str> = syms.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"primary"), "constant: {names:?}");
        assert!(names.contains(&"@card"), "class: {names:?}");
        assert!(
            names.iter().any(|n| n.contains("Default")),
            "preview: {names:?}"
        );
        // Ordered by source line: primary (l2) < @card (l4) < preview (l10).
        let lines: Vec<u32> = syms.iter().map(|s| s.range.start.line).collect();
        assert!(lines.windows(2).all(|w| w[0] <= w[1]), "sorted: {lines:?}");

        let card = syms.iter().find(|s| s.name == "@card").unwrap();
        assert_eq!(card.kind, SymbolKind::CLASS);
    }
}
