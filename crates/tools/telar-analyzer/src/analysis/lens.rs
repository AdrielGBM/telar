//! `textDocument/codeLens`: "▶ Open in workshop" over each `[preview "Name"]` header and "▶ Run play" over each `[play]`, wired to the `telar.openInWorkshop` and `telar.runPlay` commands the extension registers, which show the preview in the package's workshop and run `cargo telar test --preview <id> --no-snapshots` in the package.

use lsp_types::{CodeLens, Command, Position, Range};
use telar_parser::RsxDocument;

use crate::analysis::preview_target::{PreviewHost, PreviewTarget};

/// The command that shows a preview in the workshop, given its [`PreviewTarget`].
pub const OPEN_IN_WORKSHOP: &str = "telar.openInWorkshop";
/// The command that runs a preview's `[play]`, given its [`PreviewTarget`].
pub const RUN_PLAY: &str = "telar.runPlay";

/// The lenses shown above each `[preview]` header and each `[play]`.
pub fn code_lenses(doc: &RsxDocument, host: &PreviewHost) -> Vec<CodeLens> {
    let mut lenses = Vec::new();
    for (index, preview) in doc.previews.iter().enumerate() {
        let Some(target) = host.target(&doc.previews, index) else {
            continue;
        };
        lenses.push(lens(
            preview.line,
            "▶ Open in workshop",
            OPEN_IN_WORKSHOP,
            &target,
        ));
        if let Some(play) = &preview.play {
            lenses.push(lens(play.line, "▶ Run play", RUN_PLAY, &target));
        }
    }
    lenses
}

fn lens(line: usize, title: &str, command: &str, target: &PreviewTarget) -> CodeLens {
    let at = Position {
        line: line.saturating_sub(1) as u32,
        character: 0,
    };
    CodeLens {
        range: Range { start: at, end: at },
        command: Some(Command {
            title: title.to_string(),
            command: command.to_string(),
            arguments: Some(vec![
                serde_json::to_value(target).unwrap_or(serde_json::Value::Null),
            ]),
        }),
        data: None,
    }
}

#[cfg(test)]
#[path = "lens_test.rs"]
mod tests;
