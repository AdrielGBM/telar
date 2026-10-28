//! What names a `[preview]` outside the editor: its id, the package that builds it, and the workshops that list it. The code lenses hand it to the extension's commands, `telar/previewAt` answers it for the preview at the cursor, and `telar/workshops` answers where a file's previews are shown.

use std::path::{Path, PathBuf};

use lsp_types::TextDocumentIdentifier;
use serde::{Deserialize, Serialize};
use telar_parser::{Preview, RsxDocument, Section, is_preview_header, section_opened_by};
use telar_project::naming::{preview_id_suffix, preview_slug};
use telar_project::protocol::{package_workshop_files, preview_location};
use telar_project::{ManifestError, preview_hosts};

/// The request answering [`preview_at`] for a text document position, `null` outside a preview.
pub const PREVIEW_AT: &str = "telar/previewAt";

/// The request answering the [`Workshop`]s that list a text document's previews.
pub const WORKSHOPS: &str = "telar/workshops";

/// What [`WORKSHOPS`] is asked about.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkshopsParams {
    pub text_document: TextDocumentIdentifier,
}

/// What a file's preview ids are made of and where they are shown: `<crate>--<component>--<slug(name)>`, in the package that builds them.
pub struct PreviewHost {
    pub package_root: PathBuf,
    pub crate_name: String,
    pub component: String,
    pub workshops: Vec<Workshop>,
}

/// A package whose workshop lists a preview, and the files its running workshops are reached through: the window's channel, and the page's server, which names its address.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Workshop {
    pub package_dir: String,
    pub channel: String,
    pub server: String,
}

impl Workshop {
    fn of(package_dir: &Path) -> Option<Self> {
        let files = package_workshop_files(package_dir)?;
        Some(Self {
            package_dir: package_dir.to_string_lossy().into_owned(),
            channel: files.channel.to_string_lossy().into_owned(),
            server: files.server.to_string_lossy().into_owned(),
        })
    }
}

impl PreviewHost {
    /// The host of the `.rsx` at `rsx_path`: `None` outside a package, `Err` when a `telar.toml` deciding which workshops list it does not load. A `*.previews.rsx` previews the component it is named after, so its ids are that component's.
    pub fn discover(rsx_path: &Path) -> Option<Result<Self, ManifestError>> {
        let package_root = telar_project::find_package_root(rsx_path)?;
        let crate_name = crate::project::crate_name(&package_root)?;
        let hosts = match preview_hosts(&package_root, &crate_name) {
            Ok(hosts) => hosts,
            Err(e) => return Some(Err(e)),
        };
        Some(Ok(Self {
            workshops: hosts.iter().filter_map(|dir| Workshop::of(dir)).collect(),
            package_root,
            crate_name,
            component: telar_project::component_name(rsx_path),
        }))
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
            workshops: self.workshops.clone(),
        })
    }
}

/// A preview as the editor's commands receive it: its id, which `cargo telar preview` and `cargo telar test --preview` take; the package that builds it, where its tests run; the `goto:` location that shows it; and the workshops that list it, the one to start first.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewTarget {
    pub id: String,
    pub package_dir: String,
    pub location: String,
    pub workshops: Vec<Workshop>,
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
