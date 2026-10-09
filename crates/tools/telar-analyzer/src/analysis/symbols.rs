//! `textDocument/documentSymbol`: the `.rsx` outline / breadcrumbs.
//!
//! Surfaces the file's named, navigable symbols — `[style]` classes, the `[previews]` meta section and each `[preview]` with its `[play]` — ordered by source line. The deep `[view]` element tree is intentionally omitted: it is mostly anonymous containers and would bury the useful entries.

use lsp_types::{DocumentSymbol, Position, Range, SymbolKind};
use telar_parser::RsxDocument;

/// The document's outline: its classes, its previews meta section and its previews, each preview spanning its body and its `[play]`.
pub fn document_symbols(doc: &RsxDocument, source: &str) -> Vec<DocumentSymbol> {
    let lines: Vec<&str> = source.lines().collect();
    let mut entries: Vec<(usize, DocumentSymbol)> = Vec::new();

    for class in &doc.style.classes {
        let range = line_range(&lines, class.line);
        entries.push((
            class.line,
            symbol(format!("@{}", class.name), SymbolKind::CLASS, range, range),
        ));
    }
    let first_preview = doc.previews.first().map(|preview| preview.line);
    if let Some(meta) = &doc.previews_meta {
        let name = meta.title.clone().unwrap_or_else(|| "previews".to_string());
        let mut sym = symbol(
            name,
            SymbolKind::NAMESPACE,
            block_range(&lines, meta.line, first_preview),
            line_range(&lines, meta.line),
        );
        sym.detail = Some("previews".to_string());
        entries.push((meta.line, sym));
    }
    for (index, preview) in doc.previews.iter().enumerate() {
        let next = doc.previews.get(index + 1).map(|next| next.line);
        let mut sym = symbol(
            format!("preview: {}", preview.name),
            SymbolKind::FUNCTION,
            block_range(&lines, preview.line, next),
            line_range(&lines, preview.line),
        );
        if let Some(play) = &preview.play {
            let range = block_range(&lines, play.line, next);
            sym.children = Some(vec![symbol(
                "play".to_string(),
                SymbolKind::EVENT,
                range,
                line_range(&lines, play.line),
            )]);
        }
        entries.push((preview.line, sym));
    }

    entries.sort_by_key(|(line, _)| *line);
    entries.into_iter().map(|(_, sym)| sym).collect()
}

fn symbol(name: String, kind: SymbolKind, range: Range, selection_range: Range) -> DocumentSymbol {
    #[allow(deprecated)] // `deprecated` is a required-but-deprecated field of DocumentSymbol.
    DocumentSymbol {
        name,
        detail: None,
        kind,
        tags: None,
        deprecated: None,
        range,
        selection_range,
        children: None,
    }
}

/// The whole-line range (col 0 .. line length in UTF-16) of a 1-based line.
fn line_range(lines: &[&str], line_1based: usize) -> Range {
    block_range(lines, line_1based, Some(line_1based + 1))
}

/// From the 1-based `start` line to the last non-blank line before the 1-based `next` (or the end of the file).
fn block_range(lines: &[&str], start: usize, next: Option<usize>) -> Range {
    let start0 = start.saturating_sub(1);
    let next0 = next.map_or(lines.len(), |next| next.saturating_sub(1));
    let end0 = (start0..next0.min(lines.len()))
        .rev()
        .find(|&i| !lines[i].trim().is_empty())
        .unwrap_or(start0);
    let len = lines
        .get(end0)
        .map(|l| l.encode_utf16().count() as u32)
        .unwrap_or(0);
    Range {
        start: Position {
            line: start0 as u32,
            character: 0,
        },
        end: Position {
            line: end0 as u32,
            character: len,
        },
    }
}

#[cfg(test)]
#[path = "symbols_test.rs"]
mod tests;
