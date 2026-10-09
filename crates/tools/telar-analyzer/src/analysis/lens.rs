//! `textDocument/codeLens`: "▶ Open in workshop" over each `[preview "Name"]` header and "▶ Run play" over each `[play]`, wired to the `telar.openInWorkshop` and `telar.runPlay` commands the extension registers, which run `cargo telar preview <id>` and `cargo telar test --preview <id>` in the package.

use std::path::{Path, PathBuf};

use lsp_types::{CodeLens, Command, Position, Range};
use telar_parser::{Preview, RsxDocument};
use telar_project::naming::{preview_id_suffix, preview_slug};

/// The command that opens a preview in the workshop, given the package directory and the preview's id.
pub const OPEN_IN_WORKSHOP: &str = "telar.openInWorkshop";
/// The command that runs a preview's `[play]`, given the package directory and the preview's id.
pub const RUN_PLAY: &str = "telar.runPlay";

/// What a file's preview ids are made of and where their commands run: `<crate>--<component>--<slug(name)>`, in the package that builds them.
pub struct PreviewHost {
    pub package_root: PathBuf,
    pub crate_name: String,
    pub component: String,
}

impl PreviewHost {
    /// The host of the `.rsx` at `rsx_path`. A `*.previews.rsx` previews the component it is named after, so its ids are that component's.
    pub fn discover(rsx_path: &Path) -> Option<Self> {
        let package_root = telar_project::find_package_root(rsx_path)?;
        let crate_name = crate::project::crate_name(&package_root)?;
        Some(Self {
            package_root,
            crate_name,
            component: telar_project::component_name(rsx_path),
        })
    }

    /// The id of the `index`th preview, or `None` when the transpiler refuses it one: a name that slugs to nothing, or to the same as an earlier preview's.
    pub fn preview_id(&self, previews: &[Preview], index: usize) -> Option<String> {
        let name = &previews[index].name;
        let suffix = preview_id_suffix(&self.component, name).ok()?;
        let slug = preview_slug(name);
        let taken = previews[..index]
            .iter()
            .any(|earlier| preview_slug(&earlier.name) == slug);
        (!taken).then(|| format!("{}{suffix}", self.crate_name))
    }
}

/// The lenses shown above each `[preview]` header and each `[play]`.
pub fn code_lenses(doc: &RsxDocument, host: &PreviewHost) -> Vec<CodeLens> {
    let mut lenses = Vec::new();
    for (index, preview) in doc.previews.iter().enumerate() {
        let Some(id) = host.preview_id(&doc.previews, index) else {
            continue;
        };
        lenses.push(lens(
            preview.line,
            "▶ Open in workshop",
            OPEN_IN_WORKSHOP,
            host,
            &id,
        ));
        if let Some(play) = &preview.play {
            lenses.push(lens(play.line, "▶ Run play", RUN_PLAY, host, &id));
        }
    }
    lenses
}

fn lens(line: usize, title: &str, command: &str, host: &PreviewHost, id: &str) -> CodeLens {
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
                serde_json::Value::String(host.package_root.to_string_lossy().into_owned()),
                serde_json::Value::String(id.to_string()),
            ]),
        }),
        data: None,
    }
}

#[cfg(test)]
#[path = "lens_test.rs"]
mod tests;
