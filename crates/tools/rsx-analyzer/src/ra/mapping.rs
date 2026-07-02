use lsp_types::{
    CompletionItemKind, DiagnosticSeverity, Documentation, MarkupContent, MarkupKind, Position,
};
use ra_ap_ide::{CompletionItemKind as RaKind, LineIndex, Severity, SymbolKind, TextSize};
use ra_ap_ide_db::line_index::WideEncoding;

/// Byte offset of `(line, utf16_col)` within `text`, always landing on a UTF-8 char boundary. The column is UTF-16 (LSP convention); converting it byte-wise would point mid-character on multi-byte text and make rust-analyzer's completion panic ("start of range should be a character boundary") when it inserts its synthetic marker.
pub(super) fn byte_offset(text: &str, line: u32, utf16_col: u32) -> Option<TextSize> {
    let mut line_start = 0usize;
    for (i, current) in text.split_inclusive('\n').enumerate() {
        if i as u32 == line {
            let content = current.strip_suffix('\n').unwrap_or(current);
            let mut remaining = utf16_col;
            let mut byte = 0usize;
            for ch in content.chars() {
                let width = ch.len_utf16() as u32;
                if remaining < width {
                    break;
                }
                remaining -= width;
                byte += ch.len_utf8();
            }
            return Some(TextSize::from((line_start + byte) as u32));
        }
        line_start += current.len();
    }
    None
}

/// `None` drops a suppressed (`#[allow]`-ed) diagnostic; everything else maps to its LSP severity.
pub(super) fn map_severity(severity: Severity) -> Option<DiagnosticSeverity> {
    Some(match severity {
        Severity::Error => DiagnosticSeverity::ERROR,
        Severity::Warning => DiagnosticSeverity::WARNING,
        Severity::WeakWarning => DiagnosticSeverity::HINT,
        Severity::Allow => return None,
    })
}

/// A generated-file byte `offset` → LSP `(line, UTF-16 col)`, via the file's `LineIndex`. rust-analyzer columns are UTF-8; LSP wants UTF-16, so `to_wide` converts. An invalid offset collapses to the file start rather than panicking (the release binary is `panic="abort"`).
pub(super) fn lsp_position(line_index: &LineIndex, offset: TextSize) -> Position {
    let Some(line_col) = line_index.try_line_col(offset) else {
        return Position {
            line: 0,
            character: 0,
        };
    };
    match line_index.to_wide(WideEncoding::Utf16, line_col) {
        Some(wide) => Position {
            line: wide.line,
            character: wide.col,
        },
        None => Position {
            line: line_col.line,
            character: line_col.col,
        },
    }
}

/// rust-analyzer's rendered doc comment for a completion item → an LSP markdown `Documentation`.
pub(super) fn map_documentation(item: &ra_ap_ide::CompletionItem) -> Option<Documentation> {
    let docs = item.documentation.as_ref()?;
    Some(Documentation::MarkupContent(MarkupContent {
        kind: MarkupKind::Markdown,
        value: docs.as_str().to_string(),
    }))
}

pub(super) fn map_completion_kind(kind: RaKind) -> Option<CompletionItemKind> {
    use CompletionItemKind as L;
    Some(match kind {
        RaKind::SymbolKind(sk) => map_symbol_kind(sk),
        RaKind::Binding => L::VARIABLE,
        RaKind::BuiltinType => L::CLASS,
        RaKind::InferredType => L::VALUE,
        RaKind::Keyword => L::KEYWORD,
        RaKind::Snippet => L::SNIPPET,
        RaKind::UnresolvedReference => L::REFERENCE,
        RaKind::Expression => L::VALUE,
    })
}

fn map_symbol_kind(kind: SymbolKind) -> CompletionItemKind {
    use CompletionItemKind as L;
    match kind {
        SymbolKind::Function | SymbolKind::Macro => L::FUNCTION,
        SymbolKind::Method => L::METHOD,
        SymbolKind::Struct => L::STRUCT,
        SymbolKind::Enum => L::ENUM,
        SymbolKind::Variant => L::ENUM_MEMBER,
        SymbolKind::Field => L::FIELD,
        SymbolKind::Module => L::MODULE,
        SymbolKind::Trait => L::INTERFACE,
        SymbolKind::Const | SymbolKind::Static => L::CONSTANT,
        SymbolKind::TypeAlias => L::CLASS,
        SymbolKind::Local | SymbolKind::ValueParam | SymbolKind::SelfParam => L::VARIABLE,
        _ => L::TEXT,
    }
}
