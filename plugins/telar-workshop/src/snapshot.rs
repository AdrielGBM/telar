//! The Snapshot panel: what the workspace's last `cargo telar test` found of the selected preview's snapshot, or of the focused matrix cell's, against its golden — side by side, one over the other or swiped between, with the line diff of its draw text — and accepting a drift, which writes both goldens the way `cargo telar test --update-snapshots` does.
//!
//! A preview with a snapshot the run failed carries a badge in the sidebar, and the panel's tab counts the selected preview's.

mod panel;
mod pictures;
mod review;

pub(crate) use panel::{drift_badge, panel, tab_badge, tools};
pub(crate) use review::SnapshotReview;

use crate::state::{ViewMode, WorkshopState};

/// The id of the snapshot under review: the focused matrix cell's while the Matrix view shows one of the selected preview's cells, else the selected preview's own. Tracked.
pub(crate) fn subject(state: &WorkshopState) -> Option<String> {
    let entry = state.selected()?;
    let focused = match state.view().get() {
        ViewMode::Matrix => state
            .matrix_focus()
            .get()
            .filter(|cell| cell.starts_with(entry.id)),
        ViewMode::Canvas | ViewMode::Docs => None,
    };
    Some(focused.unwrap_or_else(|| entry.id.to_string()))
}

#[cfg(test)]
#[path = "snapshot_test.rs"]
mod tests;
