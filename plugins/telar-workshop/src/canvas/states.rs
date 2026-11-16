//! What the canvas shows in place of a preview: that there are none, or the card for one that failed.

use telar::preview::PreviewEntry;
use telar::{
    BuildFailure, LayoutError, LayoutItem, UnknownMode, Uri, UriError, UriSchemes, open_uri,
};

use super::failure_card::{FailureCardProps, failure_card};
use super::no_previews::{NoPreviewsProps, no_previews};
use crate::state::WorkshopState;
use crate::strings::{BUILD_FAILED, PANICKED};

pub(crate) fn empty() -> Result<Box<dyn LayoutItem>, LayoutError> {
    no_previews(NoPreviewsProps::props().build())
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
    card(state, entry, heading, detail, panicked)
}

/// The card a preview's canvas shows when the canvas could not be set up for it, such as for a preview asking for a mode the application has not registered.
pub(crate) fn unready(
    state: &WorkshopState,
    entry: &PreviewEntry,
    error: UnknownMode,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    card(state, entry, BUILD_FAILED, error.to_string(), false)
}

fn card(
    state: &WorkshopState,
    entry: &PreviewEntry,
    heading: &'static str,
    detail: String,
    panicked: bool,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    failure_card(
        FailureCardProps::props()
            .state(state.clone())
            .entry(*entry)
            .heading(heading)
            .detail(detail)
            .panicked(panicked)
            .build(),
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

const EDITOR_SCHEME: &str = "vscode";

/// The schemes the workshop opens [`editor_uri`]s under: the editor's own on top of the defaults. Stated where the workshop opens one rather than by the app it shows, since only the workshop builds an editor URI.
fn editor_schemes() -> UriSchemes {
    UriSchemes::default()
        .allowing(EDITOR_SCHEME)
        .expect("the editor's scheme is one an app may allow")
}

/// The URI that opens `file` at `line` in the editor the system hands it to, the path percent-encoded and with forward slashes on every platform.
pub(crate) fn editor_uri(file: &str, line: u32) -> Result<Uri, UriError> {
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
    let uri = match line {
        0 => format!("{EDITOR_SCHEME}://file/{encoded}"),
        line => format!("{EDITOR_SCHEME}://file/{encoded}:{line}"),
    };
    Uri::parse(&uri)
}

pub(crate) fn open_in_editor(file: &str, line: u32) -> bool {
    editor_uri(file, line).is_ok_and(|uri| open_uri(&uri, &editor_schemes()))
}
