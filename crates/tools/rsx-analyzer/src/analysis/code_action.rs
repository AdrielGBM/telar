//! `textDocument/codeAction`: quick fixes synthesized from the diagnostics the LSP already publishes (`rsx-diagnostics`). Each matches a diagnostic message and builds a `WorkspaceEdit` that inserts the missing symbol into `[style]`.

use std::collections::HashMap;

use lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, Diagnostic, Position, Range, TextEdit, Uri,
    WorkspaceEdit,
};
use rsx_parser::{header_section, is_preview_header};

pub fn code_actions(
    source: &str,
    uri: &Uri,
    diagnostics: &[Diagnostic],
) -> Vec<CodeActionOrCommand> {
    let mut actions = Vec::new();
    for diag in diagnostics {
        // Undefined style class → create it in `[style]`.
        if let Some(class) = between(&diag.message, "Style class `@", "`")
            && let Some(edit) = insert_into_style(source, uri, &format!("@{class}\n    "))
        {
            actions.push(quick_fix(
                format!("Create style class `@{class}` in [style]"),
                edit,
                diag,
            ));
        }
        // Unknown color reference → declare it as a `[style]` constant.
        else if let Some(color) = between(&diag.message, "Unknown color `", "`")
            && let Some(edit) = insert_into_style(source, uri, &format!("{color}: #000000"))
        {
            actions.push(quick_fix(
                format!("Add `{color}` as a [style] constant"),
                edit,
                diag,
            ));
        }
    }
    actions
}

/// The substring of `s` between the first `start` and the next `end` after it.
fn between(s: &str, start: &str, end: &str) -> Option<String> {
    let i = s.find(start)? + start.len();
    let j = s[i..].find(end)? + i;
    Some(s[i..j].to_string())
}

fn is_header(line: &str) -> bool {
    let t = line.trim();
    header_section(t).is_some() || is_preview_header(t)
}

/// A single-edit `WorkspaceEdit` that drops `snippet` into `[style]`: at the end of an existing section, or by creating one before `[view]` (or at end of file) when there is none.
fn insert_into_style(source: &str, uri: &Uri, snippet: &str) -> Option<WorkspaceEdit> {
    let lines: Vec<&str> = source.lines().collect();
    let utf16_len = |l: &str| l.encode_utf16().count() as u32;

    let (pos, text) = if let Some(style_idx) = lines.iter().position(|l| l.trim() == "[style]") {
        let next = lines
            .iter()
            .enumerate()
            .skip(style_idx + 1)
            .find(|(_, l)| is_header(l))
            .map(|(i, _)| i)
            .unwrap_or(lines.len());
        let last = (style_idx + 1..next)
            .rev()
            .find(|&j| !lines[j].trim().is_empty())
            .unwrap_or(style_idx);
        (
            Position {
                line: last as u32,
                character: utf16_len(lines[last]),
            },
            format!("\n{snippet}"),
        )
    } else if let Some(view_idx) = lines.iter().position(|l| l.trim() == "[view]") {
        (
            Position {
                line: view_idx as u32,
                character: 0,
            },
            format!("[style]\n{snippet}\n\n"),
        )
    } else {
        let last = lines.len().saturating_sub(1);
        (
            Position {
                line: last as u32,
                character: lines.get(last).map(|l| utf16_len(l)).unwrap_or(0),
            },
            format!("\n[style]\n{snippet}\n"),
        )
    };

    let edit = TextEdit {
        range: Range {
            start: pos,
            end: pos,
        },
        new_text: text,
    };
    let mut changes = HashMap::new();
    changes.insert(uri.clone(), vec![edit]);
    Some(WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    })
}

fn quick_fix(title: String, edit: WorkspaceEdit, diag: &Diagnostic) -> CodeActionOrCommand {
    CodeActionOrCommand::CodeAction(CodeAction {
        title,
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(vec![diag.clone()]),
        edit: Some(edit),
        command: None,
        is_preferred: Some(true),
        disabled: None,
        data: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn diag(message: &str) -> Diagnostic {
        Diagnostic {
            message: message.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn create_class_inserts_at_end_of_style() {
        let src = "[style]\nprimary: #ffffff\n\n@card\n    width: 240\n[view]\ncol @missing\n";
        let uri = Uri::from_str("file:///x.rsx").unwrap();
        let actions = code_actions(
            src,
            &uri,
            &[diag("Style class `@missing` is not defined in [style]")],
        );
        assert_eq!(actions.len(), 1);
        let CodeActionOrCommand::CodeAction(a) = &actions[0] else {
            panic!()
        };
        assert!(a.title.contains("@missing"), "title: {}", a.title);
        let edits = a.edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri].clone();
        assert!(edits[0].new_text.contains("@missing"));
        // Inserted after the last [style] content line (`    width: 240`, 0-based line 4).
        assert_eq!(edits[0].range.start.line, 4);
    }

    #[test]
    fn add_color_constant_quick_fix() {
        let src = "[style]\nprimary: #ffffff\n[view]\ntext color:brand\n";
        let uri = Uri::from_str("file:///x.rsx").unwrap();
        let actions = code_actions(
            src,
            &uri,
            &[diag(
                "Unknown color `brand` — not in [style] constants or theme fields",
            )],
        );
        assert_eq!(actions.len(), 1);
        let CodeActionOrCommand::CodeAction(a) = &actions[0] else {
            panic!()
        };
        let edits = a.edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri].clone();
        assert!(edits[0].new_text.contains("brand: #000000"));
    }

    #[test]
    fn no_action_for_unrelated_diagnostics() {
        let src = "[view]\ncol\n";
        let uri = Uri::from_str("file:///x.rsx").unwrap();
        assert!(code_actions(src, &uri, &[diag("some other error")]).is_empty());
    }
}
