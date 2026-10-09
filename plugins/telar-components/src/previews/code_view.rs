use telar::Children;
use telar::preview::{Layout, Matrix, PreviewEntry, preview};

use crate::code_view::{CodeSpan, CodeViewProps, TokenKind, code_view};

const SOURCE: &str = "/// Greets whoever is named.\nfn greet(name: &str) -> String {\n    let greeting = format!(\"Hello, {name}!\");\n    greeting\n}\n";

fn spans() -> Vec<CodeSpan> {
    let mut spans = Vec::new();
    let mut mark = |needle: &str, kind: TokenKind| {
        let start = SOURCE
            .find(needle)
            .expect("the sample holds every marked word");
        spans.push(CodeSpan::new(start..start + needle.len(), kind));
    };
    mark("/// Greets whoever is named.", TokenKind::Comment);
    mark("fn", TokenKind::Keyword);
    mark("greet", TokenKind::Function);
    mark("&str", TokenKind::Type);
    mark("String", TokenKind::Type);
    mark("let", TokenKind::Keyword);
    mark("format!", TokenKind::Function);
    mark("\"Hello, {name}!\"", TokenKind::String);
    spans
}

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(code_view: CodeViewProps, "Rust", |p| {
            code_view(
                CodeViewProps::props()
                    .code(SOURCE)
                    .spans(spans())
                    .first_line(p.arg("first_line", 1u32))
                    .line_numbers(p.arg("line_numbers", true))
                    .copyable(p.arg("copyable", true))
                    .highlighted(vec![p.arg("highlighted", 3u32)])
                    .max_height(p.arg("max_height", 0.0f32))
                    .build(),
                Children::default(),
            )
        })
        .title("Workbench/Code view")
        .layout(Layout::Padded)
        .matrix(Matrix::Named("themes")),
    ]
}
