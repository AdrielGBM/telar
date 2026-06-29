//! `textDocument/codeLens`: a "▶ Preview" action over each `[preview "Name"]` header, wired to the
//! `rsx.preview` command the extension registers (→ `cargo rsx preview`).

use lsp_types::{CodeLens, Command, Position, Range, Uri};
use rsx_parser::RsxDocument;

pub fn code_lenses(doc: &RsxDocument, uri: &Uri) -> Vec<CodeLens> {
    doc.previews
        .iter()
        .map(|preview| {
            let line = preview.line.saturating_sub(1) as u32;
            CodeLens {
                range: Range {
                    start: Position { line, character: 0 },
                    end: Position { line, character: 0 },
                },
                command: Some(Command {
                    title: "▶ Preview".to_string(),
                    command: "rsx.preview".to_string(),
                    arguments: Some(vec![
                        serde_json::to_value(uri).unwrap_or_default(),
                        serde_json::Value::String(preview.name.clone()),
                    ]),
                }),
                data: None,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsx_parser::parse;
    use std::str::FromStr;

    #[test]
    fn one_lens_per_preview_on_its_header_line() {
        let src = "[view]\ncol\n\n[preview \"A\"]\ncol\n\n[preview \"B\"]\nbox\n";
        let doc = parse(src).unwrap();
        let uri = Uri::from_str("file:///x.rsx").unwrap();
        let lenses = code_lenses(&doc, &uri);
        assert_eq!(lenses.len(), 2);
        // Header lines are 0-based 3 and 6.
        assert_eq!(lenses[0].range.start.line, 3);
        assert_eq!(lenses[1].range.start.line, 6);
        let cmd = lenses[0].command.as_ref().unwrap();
        assert_eq!(cmd.command, "rsx.preview");
        assert_eq!(cmd.arguments.as_ref().unwrap()[1], serde_json::json!("A"));
    }
}
