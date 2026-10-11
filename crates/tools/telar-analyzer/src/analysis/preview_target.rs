//! What names a `[preview]` outside the editor: its id, the package that builds it, and where that package's running workshop listens. The code lenses hand it to the extension's commands, and `telar/previewAt` answers it for the preview at the cursor.

use std::path::{Path, PathBuf};

use serde::Serialize;
use telar_parser::{Preview, RsxDocument, Section, is_preview_header, section_opened_by};
use telar_project::naming::{preview_id_suffix, preview_slug};
use telar_project::protocol::{package_workshop_channel_file, preview_location};

/// The request answering [`preview_at`] for a text document position, `null` outside a preview.
pub const PREVIEW_AT: &str = "telar/previewAt";

/// What a file's preview ids are made of and where their commands run: `<crate>--<component>--<slug(name)>`, in the package that builds them.
pub struct PreviewHost {
    pub package_root: PathBuf,
    pub crate_name: String,
    pub component: String,
    pub workshop_channel: Option<PathBuf>,
}

impl PreviewHost {
    /// The host of the `.rsx` at `rsx_path`. A `*.previews.rsx` previews the component it is named after, so its ids are that component's.
    pub fn discover(rsx_path: &Path) -> Option<Self> {
        let package_root = telar_project::find_package_root(rsx_path)?;
        let crate_name = crate::project::crate_name(&package_root)?;
        Some(Self {
            workshop_channel: package_workshop_channel_file(&package_root),
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

    /// The target of the `index`th preview, or `None` when it has no id.
    pub fn target(&self, previews: &[Preview], index: usize) -> Option<PreviewTarget> {
        let id = self.preview_id(previews, index)?;
        Some(PreviewTarget {
            location: preview_location(&id),
            id,
            package_dir: self.package_root.to_string_lossy().into_owned(),
            channel: self
                .workshop_channel
                .as_ref()
                .map(|file| file.to_string_lossy().into_owned()),
        })
    }
}

/// A preview as the editor's commands receive it: what `cargo telar preview` and `cargo telar test --preview` take, where to run them, and the `goto:` location to send a workshop already running on `channel`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewTarget {
    pub id: String,
    pub package_dir: String,
    pub location: String,
    pub channel: Option<String>,
}

/// The target of the preview whose zone — its header, its body and its `[play]` — holds 0-based `line` of `source`, which `doc` must be the parse of.
pub fn preview_at(
    doc: &RsxDocument,
    source: &str,
    line: u32,
    host: &PreviewHost,
) -> Option<PreviewTarget> {
    let header_line = preview_header_above(source, line as usize)?;
    let index = doc
        .previews
        .iter()
        .position(|preview| preview.line == header_line)?;
    host.target(&doc.previews, index)
}

/// The 1-based line of the `[preview …]` header whose zone holds 0-based `line`, or `None` when another section opened since.
fn preview_header_above(source: &str, line: usize) -> Option<usize> {
    let mut header = None;
    for (index, text) in source.lines().enumerate().take(line + 1) {
        let trimmed = text.trim();
        if is_preview_header(trimmed) {
            header = Some(index + 1);
        } else if section_opened_by(trimmed).is_some_and(|opened| opened != Section::Play) {
            header = None;
        }
    }
    header
}

#[cfg(test)]
#[path = "preview_target_test.rs"]
mod tests;
