//! What the canvas shows in place of a preview: that there are none, or the card for one that failed.

use telar::preview::PreviewEntry;
use telar::{BuildFailure, Children, LayoutError, LayoutItem};

use super::failure_card::{FailureCardProps, failure_card};
use super::no_previews::{NoPreviewsProps, no_previews};
use crate::state::WorkshopState;
use crate::strings::{BUILD_FAILED, PANICKED};

pub(crate) fn empty() -> Result<Box<dyn LayoutItem>, LayoutError> {
    no_previews(NoPreviewsProps::props().build(), Children::default())
}

/// The card a preview's canvas shows when building it failed, in the workbench theme rather than the application's.
pub(crate) fn failure(
    state: &WorkshopState,
    entry: &PreviewEntry,
    failure: BuildFailure,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (heading, detail, panicked) = match failure {
        BuildFailure::Failed(err) => (BUILD_FAILED, err.to_string(), false),
        BuildFailure::Panicked(message) => (PANICKED, message, true),
    };
    failure_card(
        FailureCardProps::props()
            .state(state.clone())
            .entry(*entry)
            .heading(heading)
            .detail(detail)
            .panicked(panicked)
            .build(),
        Children::default(),
    )
}

/// `file:line`, or `None` when the preview's file is unknown.
pub(super) fn location(entry: &PreviewEntry) -> Option<String> {
    match (entry.file, entry.line) {
        ("", _) => None,
        (file, 0) => Some(file.to_owned()),
        (file, line) => Some(format!("{file}:{line}")),
    }
}

/// The URI that opens `file` at `line` in the editor the system hands it to, the path percent-encoded and with forward slashes on every platform.
pub(crate) fn editor_uri(file: &str, line: u32) -> String {
    let path = file.replace('\\', "/");
    let mut encoded = String::with_capacity(path.len());
    for byte in path.trim_start_matches('/').bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' | b':' => {
                encoded.push(byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    match line {
        0 => format!("vscode://file/{encoded}"),
        line => format!("vscode://file/{encoded}:{line}"),
    }
}
