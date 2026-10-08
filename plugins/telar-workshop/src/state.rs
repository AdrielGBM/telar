//! [`WorkshopState`]: what the workshop's regions share, and what of it survives a hot reload.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use telar::focus::{self, FocusId};
use telar::preview::PreviewEntry;
use telar::preview::host::{Args, PreviewRequest};
use telar::{OwnerId, RwSignal, hot_signal, owner_scope, signal, with_owner};

use crate::sidebar::group_ids;

/// The `hot_signal` keys the workshop's state is kept under. A hot reload restores each by key, so renaming one forgets what it held.
pub(crate) mod keys {
    pub(crate) const SELECTION: &str = "@workshop/selection";
    pub(crate) const VIEW: &str = "@workshop/view";
    pub(crate) const SEARCH: &str = "@workshop/search";
    pub(crate) const SIDEBAR_EXPANDED: &str = "@workshop/sidebar.expanded";
    pub(crate) const SIDEBAR_WIDTH: &str = "@workshop/sidebar.width";
    pub(crate) const SIDEBAR_COLLAPSED: &str = "@workshop/sidebar.collapsed";
    pub(crate) const PANEL_SIZE: &str = "@workshop/panel.size";
    pub(crate) const PANEL_COLLAPSED: &str = "@workshop/panel.collapsed";
    pub(crate) const PANEL_POSITION: &str = "@workshop/panel.position";
    pub(crate) const CANVAS_VIEWPORT: &str = "@workshop/canvas.viewport";
}

pub(crate) const SIDEBAR_DEFAULT_WIDTH: f32 = 240.0;
pub(crate) const SIDEBAR_MIN_WIDTH: f32 = 180.0;
pub(crate) const SIDEBAR_MAX_WIDTH: f32 = 400.0;
pub(crate) const PANEL_DEFAULT_SIZE: f32 = 240.0;
pub(crate) const PANEL_MIN_SIZE: f32 = 160.0;
pub(crate) const PANEL_MAX_FRACTION: f32 = 0.6;

/// Which of the workshop's views the area beside the sidebar shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum ViewMode {
    #[default]
    Canvas,
    Matrix,
    Docs,
}

/// Which edge of the canvas the panels sit along.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum PanelPosition {
    #[default]
    Bottom,
    Right,
}

/// What the sidebar, the canvas and the panels share: the previews, which one is selected, which groups are open, the pane sizes, the view, the search and each preview's args.
///
/// A cheap handle: clones share one state. Everything but the previews, the remount count and the search field is a `@workshop/…` hot signal, so it is made where the tree is built, after a hot reload has restored the values it carries.
#[derive(Clone)]
pub(crate) struct WorkshopState {
    entries: Rc<[PreviewEntry]>,
    selection: RwSignal<Option<Arc<str>>>,
    expanded: RwSignal<HashSet<Arc<str>>>,
    view: RwSignal<ViewMode>,
    search: RwSignal<String>,
    sidebar_width: RwSignal<f32>,
    sidebar_collapsed: RwSignal<bool>,
    panel_size: RwSignal<f32>,
    panel_collapsed: RwSignal<bool>,
    panel_position: RwSignal<PanelPosition>,
    viewport: RwSignal<Option<(f32, f32)>>,
    remounts: RwSignal<u64>,
    search_field: Rc<Cell<Option<FocusId>>>,
    args: Rc<RefCell<HashMap<&'static str, Args>>>,
    // Args are made lazily, often from inside a canvas, and would otherwise be freed with whichever canvas asked first.
    owner: OwnerId,
}

impl WorkshopState {
    /// The state for `entries`, selecting what a hot reload restored while it still names a preview, and otherwise the preview `requested` resolves to, or else the first.
    pub(crate) fn new(entries: Rc<[PreviewEntry]>, requested: &PreviewRequest) -> Self {
        let listed = |id: &str| entries.iter().any(|entry| entry.id == id);
        let initial = requested
            .resolve(&entries)
            .or_else(|| entries.first())
            .map(|entry| Arc::from(entry.id));
        let selection = hot_signal(keys::SELECTION, initial.clone());
        if !selection.peek().is_some_and(|id| listed(&id)) {
            selection.set(initial);
        }
        let every_group = group_ids(&entries).into_iter().collect();
        Self {
            selection,
            expanded: hot_signal(keys::SIDEBAR_EXPANDED, every_group),
            view: hot_signal(keys::VIEW, ViewMode::default()),
            search: hot_signal(keys::SEARCH, String::new()),
            sidebar_width: hot_signal(keys::SIDEBAR_WIDTH, SIDEBAR_DEFAULT_WIDTH),
            sidebar_collapsed: hot_signal(keys::SIDEBAR_COLLAPSED, false),
            panel_size: hot_signal(keys::PANEL_SIZE, PANEL_DEFAULT_SIZE),
            panel_collapsed: hot_signal(keys::PANEL_COLLAPSED, false),
            panel_position: hot_signal(keys::PANEL_POSITION, PanelPosition::default()),
            viewport: hot_signal(keys::CANVAS_VIEWPORT, None),
            remounts: signal(0),
            search_field: Rc::default(),
            args: Rc::default(),
            owner: owner_scope().id(),
            entries,
        }
    }

    pub(crate) fn entries(&self) -> &[PreviewEntry] {
        &self.entries
    }

    /// The selected preview's id. `None` only when there are no previews.
    pub(crate) fn selection(&self) -> RwSignal<Option<Arc<str>>> {
        self.selection
    }

    /// The selected preview, subscribing the caller to the selection.
    pub(crate) fn selected(&self) -> Option<PreviewEntry> {
        let id = self.selection.get()?;
        self.entries.iter().find(|entry| *entry.id == *id).copied()
    }

    /// Selects the preview `id` names, and answers whether one does; an id no preview has leaves the selection as it was.
    pub(crate) fn select(&self, id: &str) -> bool {
        let listed = self.entries.iter().any(|entry| entry.id == id);
        if listed && self.selection.peek().as_deref() != Some(id) {
            self.selection.set(Some(Arc::from(id)));
        }
        listed
    }

    /// The groups of the sidebar that are open, by the id the sidebar gives each. Every group starts open.
    pub(crate) fn expanded(&self) -> RwSignal<HashSet<Arc<str>>> {
        self.expanded
    }

    pub(crate) fn view(&self) -> RwSignal<ViewMode> {
        self.view
    }

    pub(crate) fn search(&self) -> RwSignal<String> {
        self.search
    }

    /// Takes the keyboard to the search field, opening the sidebar first if it is collapsed.
    pub(crate) fn focus_search(&self) {
        self.sidebar_collapsed.set(false);
        if let Some(field) = self.search_field.get() {
            focus::request(field);
        }
    }

    /// Names the field [`Self::focus_search`] takes the keyboard to.
    pub(crate) fn set_search_field(&self, field: FocusId) {
        self.search_field.set(Some(field));
    }

    /// In logical px. The shell keeps it within [`SIDEBAR_MIN_WIDTH`]..=[`SIDEBAR_MAX_WIDTH`] whatever it holds.
    pub(crate) fn sidebar_width(&self) -> RwSignal<f32> {
        self.sidebar_width
    }

    /// Whether the sidebar is folded away. Its width is kept for when it opens again.
    pub(crate) fn sidebar_collapsed(&self) -> RwSignal<bool> {
        self.sidebar_collapsed
    }

    /// The panels' height when they sit below the canvas and their width when they sit beside it, in logical px. The shell keeps it at least [`PANEL_MIN_SIZE`] and at most [`PANEL_MAX_FRACTION`] of the area it shares with the canvas.
    pub(crate) fn panel_size(&self) -> RwSignal<f32> {
        self.panel_size
    }

    /// Whether the panels are folded away. Their size is kept for when they open again.
    pub(crate) fn panel_collapsed(&self) -> RwSignal<bool> {
        self.panel_collapsed
    }

    pub(crate) fn panel_position(&self) -> RwSignal<PanelPosition> {
        self.panel_position
    }

    /// The canvas size the viewport handles set, as `(width, height)` in logical px, for every preview alike. `None` leaves each preview at the size it asks for, or filling the canvas pane.
    pub(crate) fn viewport(&self) -> RwSignal<Option<(f32, f32)>> {
        self.viewport
    }

    /// Mounts the selected preview afresh, on a new canvas.
    pub(crate) fn remount(&self) {
        self.remounts.update(|count| *count = count.wrapping_add(1));
    }

    /// How many times [`Self::remount`] was asked for. Tracked.
    pub(crate) fn remounts(&self) -> u64 {
        self.remounts.get()
    }

    /// The args `entry`'s canvas mounts it with and its controls list: one handle per preview for as long as the workshop lives, so both sides share it, with the overrides kept across a hot reload under the preview's id.
    pub(crate) fn args(&self, entry: &PreviewEntry) -> Args {
        self.args
            .borrow_mut()
            .entry(entry.id)
            .or_insert_with(|| with_owner(Some(self.owner), || Args::for_entry(entry)))
            .clone()
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod tests;
