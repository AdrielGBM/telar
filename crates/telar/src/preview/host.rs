//! Hosting previews: the [`Args`] behind each canvas that a controls panel lists and sets, the mount that builds a preview again when an arg it read changes, and the hosts `cargo telar` starts. A preview's author needs none of it.

use std::collections::BTreeMap;

use super::PreviewEntry;

pub use super::args::{ArgError, ArgState, Args};
pub use super::mount::remounting;

#[cfg(all(
    any(feature = "preview", feature = "preview-headless"),
    not(target_os = "android")
))]
pub use super::app::PreviewApp;
#[cfg(all(feature = "preview-headless", not(target_os = "android")))]
pub use super::app::run_preview_png;
#[cfg(not(target_os = "android"))]
pub use super::runner::{dev_entry, try_run_test};

/// What is wrong with `entries` when ids repeat, one line per repeated id naming where each of its previews is written, or `None` when every id is its own. An id names a deep link and a snapshot file, so two previews under one would overwrite each other.
pub(crate) fn duplicate_ids(entries: &[PreviewEntry]) -> Option<String> {
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
