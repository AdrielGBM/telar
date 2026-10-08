//! Hosting previews: the [`Args`] behind each canvas that a controls panel lists and sets, the mount that builds a preview again when an arg it read changes, and the hosts `cargo telar` starts. A preview's author needs none of it.

use std::collections::BTreeMap;

use super::PreviewEntry;

pub use super::args::{ArgError, ArgState, Args};
pub use super::mount::remounting;

#[cfg(not(target_os = "android"))]
pub use super::app::PreviewApp;
#[cfg(all(feature = "preview-headless", not(target_os = "android")))]
pub use super::app::run_preview_png;
#[cfg(not(target_os = "android"))]
pub use super::runner::{dev_entry, try_run_test};

/// The env var naming the preview to open first.
pub const PREVIEW_ID_VAR: &str = "TELAR_PREVIEW_ID";

/// The env var `cargo telar preview --component` sets to the component to scope the host to.
pub const PREVIEW_COMPONENT_VAR: &str = "TELAR_PREVIEW_COMPONENT";

/// What `cargo telar preview` asked a host to open: a preview by id, a component by name, or neither.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PreviewRequest {
    pub id: Option<String>,
    pub component: Option<String>,
}

impl PreviewRequest {
    /// The request `id` and `component` make, each `None` when absent or blank.
    pub fn new(id: Option<&str>, component: Option<&str>) -> Self {
        let named = |value: Option<&str>| {
            value
                .filter(|value| !value.trim().is_empty())
                .map(str::to_owned)
        };
        Self {
            id: named(id),
            component: named(component),
        }
    }

    /// Whether `entry` belongs to the requested component, by its `component` or by the last segment of its title. Any entry matches when no component was requested.
    pub fn matches_component(&self, entry: &PreviewEntry) -> bool {
        self.component.as_deref().is_none_or(|wanted| {
            entry.component == wanted || entry.title.rsplit('/').next() == Some(wanted)
        })
    }

    /// The preview to open first among `entries`: the one `id` names, otherwise the first of the requested component. `None` when neither is among them.
    pub fn resolve<'a>(&self, entries: &'a [PreviewEntry]) -> Option<&'a PreviewEntry> {
        let by_id = self
            .id
            .as_deref()
            .and_then(|id| entries.iter().find(|entry| entry.id == id));
        by_id.or_else(|| {
            self.component
                .as_ref()
                .and_then(|_| entries.iter().find(|entry| self.matches_component(entry)))
        })
    }
}

/// The request in `TELAR_PREVIEW_ID` and `TELAR_PREVIEW_COMPONENT`.
///
/// A shell reads it once, as it starts: a selection it restored across a hot reload wins over it.
pub fn requested_preview() -> PreviewRequest {
    PreviewRequest::new(
        std::env::var(PREVIEW_ID_VAR).ok().as_deref(),
        std::env::var(PREVIEW_COMPONENT_VAR).ok().as_deref(),
    )
}

/// What is wrong with `entries` when ids repeat, one line per repeated id naming where each of its previews is written, or `None` when every id is its own. An id names a deep link and a snapshot file, so two previews under one would overwrite each other.
pub fn duplicate_ids(entries: &[PreviewEntry]) -> Option<String> {
    let mut by_id: BTreeMap<&str, Vec<&PreviewEntry>> = BTreeMap::new();
    for entry in entries {
        by_id.entry(entry.id).or_default().push(entry);
    }
    let lines: Vec<String> = by_id
        .into_iter()
        .filter(|(_, shared)| shared.len() > 1)
        .map(|(id, shared)| {
            let places: Vec<String> = shared.iter().map(|entry| place(entry)).collect();
            format!(
                "preview id `{id}` names {} previews ({}); rename all but one",
                shared.len(),
                places.join(", ")
            )
        })
        .collect();
    (!lines.is_empty()).then(|| lines.join("\n"))
}

/// Prints a `FAIL` line for each id `entries` repeat, the way a run that renders or tests every preview reports one that failed, and answers how many it printed.
#[cfg(not(target_os = "android"))]
pub(crate) fn fail_duplicate_ids(entries: &[PreviewEntry]) -> usize {
    let Some(duplicates) = duplicate_ids(entries) else {
        return 0;
    };
    duplicates
        .lines()
        .inspect(|line| println!("  FAIL  {line}"))
        .count()
}

fn place(entry: &PreviewEntry) -> String {
    match entry.file {
        "" => format!("\"{}\"", entry.name),
        file => format!("{file}:{}", entry.line),
    }
}

#[cfg(test)]
#[path = "host_test.rs"]
mod tests;
