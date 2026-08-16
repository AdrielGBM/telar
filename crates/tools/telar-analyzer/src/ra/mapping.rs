use lsp_types::{
    CompletionItemKind, DiagnosticSeverity, Documentation, MarkupContent, MarkupKind, Position,
};
use ra_ap_ide::{CompletionItemKind as RaKind, LineIndex, Severity, SymbolKind, TextSize};
use ra_ap_ide_db::line_index::WideEncoding;

/// `None` drops a suppressed (`#[allow]`-ed) diagnostic; everything else maps to its LSP severity.
pub(super) fn map_severity(severity: Severity) -> Option<DiagnosticSeverity> {
    Some(match severity {
        Severity::Error => DiagnosticSeverity::ERROR,
        Severity::Warning => DiagnosticSeverity::WARNING,
        Severity::WeakWarning => DiagnosticSeverity::HINT,
        Severity::Allow => return None,
    })
}

/// A generated-file byte `offset` → LSP `(line, UTF-16 col)`, via the file's `LineIndex`. rust-analyzer columns are UTF-8; LSP wants UTF-16, so `to_wide` converts. An invalid offset collapses to the file start rather than panicking, so a stale index cannot take the language server down mid-session.
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
