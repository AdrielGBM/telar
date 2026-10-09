//! [`WorkshopState`]: what the workshop's regions share, and what of it survives a hot reload.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use telar::focus::{self, FocusId};
use telar::preview::host::{Args, PreviewRequest};
use telar::preview::{ActionLog, ArgValue, PreviewEntry};
use telar::{OwnerId, Route, RwSignal, batch, hot_signal, owner_scope, signal, with_owner};

use crate::route::{Link, Page, WorkshopRoute};
use crate::settings::CanvasSettings;
use crate::sidebar::{group_ids, listed};
use crate::store::{self, Keeper};

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
    pub(crate) const PANEL_TAB: &str = "@workshop/panel.tab";
    pub(crate) const ACTIONS_FILTER: &str = "@workshop/actions.filter";
    pub(crate) const CANVAS_SETTINGS: &str = "@workshop/canvas.settings";
    pub(crate) const CHROME: &str = "@workshop/chrome";
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
    Docs,
}

/// Which edge of the canvas the panels sit along.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum PanelPosition {
    #[default]
    Bottom,
    Right,
}

/// Which panel the strip along the canvas shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum PanelTab {
    #[default]
    Controls,
    Actions,
}

/// What the sidebar, the canvas and the panels share: the previews, which one is selected, which groups are open, the pane sizes, the view, the search, the panel shown and the actions filter, each preview's args, the canvas toolbar's settings and the calls the mounted preview's callbacks received.
///
/// A cheap handle: clones share one state. Everything but the previews, the remount count, the action log and the search field is a `@workshop/…` hot signal, so it is made where the tree is built, after a hot reload has restored the values it carries.
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
    panel_tab: RwSignal<PanelTab>,
    actions_filter: RwSignal<String>,
    canvas_settings: RwSignal<CanvasSettings>,
    chrome: RwSignal<bool>,
    remounts: RwSignal<u64>,
    actions: ActionLog,
    search_field: Rc<Cell<Option<FocusId>>>,
    args: Rc<RefCell<HashMap<&'static str, Args>>>,
    // Args are made lazily, often from inside a canvas, and would otherwise be freed with whichever canvas asked first.
    owner: OwnerId,
}

impl WorkshopState {
    /// [`Self::kept_in`] keeping nothing between runs: the state a test builds.
    #[cfg(test)]
    pub(crate) fn new(entries: Rc<[PreviewEntry]>, requested: &PreviewRequest) -> Self {
        Self::kept_in(entries, requested, &Keeper::none())
    }

    /// The state for `entries`, with what a hot reload restored, and otherwise what `keeper` kept on the last run: the selection, the view, the pane sizes and the canvas toolbar's settings, each kept there again as it changes.
    ///
    /// The selection a hot reload restored wins while it still names a preview. Otherwise it is the preview `requested` names by id, else the stored one, else the first; where the requested component, read as a title filter, keeps any previews, the stored and the first are chosen among those. The search starts as that filter.
    pub(crate) fn kept_in(
        entries: Rc<[PreviewEntry]>,
        requested: &PreviewRequest,
        keeper: &Keeper,
    ) -> Self {
        let filter = requested.component.clone().unwrap_or_default();
        let filtered = match filter.is_empty() {
            true => Vec::new(),
            false => listed(&entries, &filter),
        };
        let shown = match filtered.is_empty() {
            true => entries.to_vec(),
            false => filtered,
        };
        let by_id = |id: &str| entries.iter().find(|entry| entry.id == id);
        let initial = requested
            .id
            .as_deref()
            .and_then(by_id)
            .or_else(|| {
                let stored: Arc<str> = keeper.stored(store::keys::SELECTION)?;
                shown.iter().find(|entry| *entry.id == *stored)
            })
            .or_else(|| shown.first())
            .map(|entry| Arc::from(entry.id));
        let selection = hot_signal(keys::SELECTION, initial.clone());
        if !selection.peek().is_some_and(|id| by_id(&id).is_some()) {
            selection.set(initial);
        }
        keeper.keep(selection, store::keys::SELECTION);
        let every_group = group_ids(&entries).into_iter().collect();
        Self {
            selection,
            expanded: hot_signal(keys::SIDEBAR_EXPANDED, every_group),
            view: keeper.kept(keys::VIEW, store::keys::VIEW, ViewMode::default()),
            search: hot_signal(keys::SEARCH, filter),
            sidebar_width: keeper.kept(
                keys::SIDEBAR_WIDTH,
                store::keys::SIDEBAR_WIDTH,
                SIDEBAR_DEFAULT_WIDTH,
            ),
            sidebar_collapsed: keeper.kept(
                keys::SIDEBAR_COLLAPSED,
                store::keys::SIDEBAR_COLLAPSED,
                false,
            ),
            panel_size: keeper.kept(
                keys::PANEL_SIZE,
                store::keys::PANEL_SIZE,
                PANEL_DEFAULT_SIZE,
            ),
            panel_collapsed: keeper.kept(
                keys::PANEL_COLLAPSED,
                store::keys::PANEL_COLLAPSED,
                false,
            ),
            panel_position: keeper.kept(
                keys::PANEL_POSITION,
                store::keys::PANEL_POSITION,
                PanelPosition::default(),
            ),
            panel_tab: hot_signal(keys::PANEL_TAB, PanelTab::default()),
            actions_filter: hot_signal(keys::ACTIONS_FILTER, String::new()),
            canvas_settings: keeper.kept(
                keys::CANVAS_SETTINGS,
                store::keys::CANVAS_SETTINGS,
                CanvasSettings::default(),
            ),
            chrome: hot_signal(keys::CHROME, true),
            remounts: signal(0),
            actions: ActionLog::new(),
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

    pub(crate) fn panel_tab(&self) -> RwSignal<PanelTab> {
        self.panel_tab
    }

    /// What the actions panel narrows the log to: the calls whose name or arguments contain it, ignoring case.
    pub(crate) fn actions_filter(&self) -> RwSignal<String> {
        self.actions_filter
    }

    /// What the canvas toolbar and the viewport handles set, for every preview alike.
    pub(crate) fn canvas_settings(&self) -> RwSignal<CanvasSettings> {
        self.canvas_settings
    }

    /// Whether the chrome shows around the canvas; without it the workshop is the canvas alone, as an embedding or an editor panel shows it.
    pub(crate) fn chrome(&self) -> RwSignal<bool> {
        self.chrome
    }

    /// Where the workshop is, as its address carries it, subscribing the caller to the selection, the view and the chrome. `None` only when there are no previews.
    pub(crate) fn route(&self) -> Option<WorkshopRoute> {
        let entry = self.selected()?;
        Some(route_of(&entry, self.view.get(), self.chrome.get()))
    }

    /// [`Self::route`] without subscribing.
    pub(crate) fn current_route(&self) -> Option<WorkshopRoute> {
        let id = self.selection.peek()?;
        let entry = self.entries.iter().find(|entry| *entry.id == *id)?;
        Some(route_of(entry, self.view.peek(), self.chrome.peek()))
    }

    /// Goes where `route` points, and sets what its link carries: the canvas toolbar's settings, and the args of the preview it lands on, every other arg back at its default. An id or a title no preview has leaves the selection as it was; an arg the preview no longer has, or a value it no longer takes, is left at its default.
    pub(crate) fn open(&self, route: &WorkshopRoute) {
        batch(|| {
            let (view, listed) = match &route.page {
                Page::Preview(id) => (ViewMode::Canvas, self.select(id)),
                Page::Docs(title) => (ViewMode::Docs, self.select_title(title)),
            };
            if listed {
                self.view.set_if_changed(view);
            }
            self.chrome.set_if_changed(route.chrome);
            let Some(Link { args, settings }) = &route.link else {
                return;
            };
            if let Some(settings) = settings {
                self.canvas_settings.set(settings.clone());
            }
            if let Some(entry) = listed.then(|| self.current_entry()).flatten() {
                let held = self.args(&entry);
                held.reset();
                for (name, value) in args {
                    let _ = held.set(name, value.clone());
                }
            }
        });
    }

    /// A link to where the workshop is now that also carries the selected preview's edited args and the canvas toolbar's settings: what "Copy link" puts on the clipboard, which `cargo telar preview <link>` opens again.
    pub(crate) fn link(&self) -> Option<String> {
        let mut route = self.current_route()?;
        let entry = self.current_entry()?;
        route.link = Some(Link {
            args: edited_args(&self.args(&entry)),
            settings: Some(self.canvas_settings.peek()),
        });
        Some(telar::location_format().format(&route.to_location()))
    }

    /// Puts [`Self::link`] on the clipboard.
    pub(crate) fn copy_link(&self) {
        if let Some(link) = self.link() {
            telar::set_clipboard_text(&link);
        }
    }

    fn current_entry(&self) -> Option<PreviewEntry> {
        let id = self.selection.peek()?;
        self.entries.iter().find(|entry| *entry.id == *id).copied()
    }

    /// Selects the first preview listed under `title`, unless the selected one already is.
    fn select_title(&self, title: &str) -> bool {
        if self
            .current_entry()
            .is_some_and(|entry| entry.title == title)
        {
            return true;
        }
        let first = self.entries.iter().find(|entry| entry.title == title);
        first.is_some_and(|entry| self.select(entry.id))
    }

    /// Mounts the selected preview afresh, on a new canvas.
    pub(crate) fn remount(&self) {
        self.remounts.update(|count| *count = count.wrapping_add(1));
    }

    /// How many times [`Self::remount`] was asked for. Tracked.
    pub(crate) fn remounts(&self) -> u64 {
        self.remounts.get()
    }

    /// The calls the mounted preview's callbacks received. One log for as long as the workshop lives, so a panel reading it outlives each canvas, and emptied as each canvas mounts.
    pub(crate) fn actions(&self) -> ActionLog {
        self.actions
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

fn route_of(entry: &PreviewEntry, view: ViewMode, chrome: bool) -> WorkshopRoute {
    let page = match view {
        ViewMode::Canvas => Page::Preview(entry.id.into()),
        ViewMode::Docs => Page::Docs(entry.title.into()),
    };
    WorkshopRoute::new(page, chrome)
}

/// What a link carries of `args`: each value that differs from its default, by name, in name order, including one held for an arg the preview has not read yet.
fn edited_args(args: &Args) -> Vec<(String, ArgValue)> {
    let mut edited: Vec<(String, ArgValue)> = args
        .states()
        .into_iter()
        .filter(|state| state.edited)
        .filter_map(|state| Some((state.name.to_string(), state.value?)))
        .collect();
    for (name, value) in args.overrides() {
        if !edited.iter().any(|(held, _)| *held == name) {
            edited.push((name, value));
        }
    }
    edited.sort_by(|(a, _), (b, _)| a.cmp(b));
    edited
}

#[cfg(test)]
#[path = "state_test.rs"]
mod tests;
