//! [`WorkshopState`]: what the workshop's regions share, and what of it survives a hot reload.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use telar::focus::{self, FocusId};
use telar::preview::host::{Args, PreviewRequest, docs_key, docs_path, mount_preview, title_path};
use telar::preview::{ActionLog, ArgValue, PreviewCtx, PreviewEntry};
use telar::{
    Container, LayoutError, LayoutItem, LayoutStyle, Memo, OwnerId, Route, RwSignal, Size,
    SurfaceCanvas, batch, box_item, effect, hot_signal, owner_scope, signal, with_owner,
};
use telar_devtools::Inspection;

use crate::arrangement::Arrangement;
use crate::interactions::Interactions;
use crate::matrix::MatrixAxes;
use crate::route::{Page, WorkshopRoute, cell_past};
use crate::settings::CanvasSettings;
use crate::sidebar::{group_ids, groups_holding, listed};
use crate::snapshot::SnapshotReview;
use crate::store::{self, Keeper};

/// The `hot_signal` keys the workshop's state is kept under. A hot reload restores each by key, so renaming one forgets what it held.
pub(crate) mod keys {
    pub(crate) const SELECTION: &str = "@workshop/selection";
    pub(crate) const VIEW: &str = "@workshop/view";
    pub(crate) const SEARCH: &str = "@workshop/search";
    pub(crate) const SIDEBAR_EXPANDED: &str = "@workshop/sidebar.expanded";
    pub(crate) const SIDEBAR_RESTED: &str = "@workshop/sidebar.rested";
    pub(crate) const SIDEBAR_WIDTH: &str = "@workshop/sidebar.width";
    pub(crate) const SIDEBAR_COLLAPSED: &str = "@workshop/sidebar.collapsed";
    pub(crate) const PANEL_SIZE: &str = "@workshop/panel.size";
    pub(crate) const PANEL_COLLAPSED: &str = "@workshop/panel.collapsed";
    pub(crate) const PANEL_POSITION: &str = "@workshop/panel.position";
    pub(crate) const PANEL_TAB: &str = "@workshop/panel.tab";
    pub(crate) const ACTIONS_FILTER: &str = "@workshop/actions.filter";
    pub(crate) const CANVAS_SETTINGS: &str = "@workshop/canvas.settings";
    pub(crate) const CHROME: &str = "@workshop/chrome";
    pub(crate) const MATRIX_AXES: &str = "@workshop/matrix.axes";
    pub(crate) const MATRIX_FOCUS: &str = "@workshop/matrix.focus";
    pub(crate) const PLAY_AUTO_RUN: &str = "@workshop/play.auto_run";
    pub(crate) const SNAPSHOT_MODE: &str = "@workshop/snapshot.mode";
}

pub(crate) const SIDEBAR_DEFAULT_WIDTH: f32 = 240.0;
pub(crate) const SIDEBAR_MIN_WIDTH: f32 = 180.0;
pub(crate) const SIDEBAR_MAX_WIDTH: f32 = 400.0;
pub(crate) const PANEL_DEFAULT_SIZE: f32 = 240.0;
pub(crate) const PANEL_MIN_SIZE: f32 = 160.0;
pub(crate) const PANEL_MAX_FRACTION: f32 = 0.6;

/// Whether each view folds the panels down to their tabs: the canvas shows them, while a matrix and a docs page have panels of little use to them and the room to spare.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct FoldedViews {
    canvas: bool,
    matrix: bool,
    docs: bool,
}

impl Default for FoldedViews {
    fn default() -> Self {
        Self {
            canvas: false,
            matrix: true,
            docs: true,
        }
    }
}

impl FoldedViews {
    fn of(&self, view: ViewMode) -> bool {
        match view {
            ViewMode::Canvas => self.canvas,
            ViewMode::Matrix => self.matrix,
            ViewMode::Docs => self.docs,
        }
    }

    fn set(&mut self, view: ViewMode, folded: bool) {
        match view {
            ViewMode::Canvas => self.canvas = folded,
            ViewMode::Matrix => self.matrix = folded,
            ViewMode::Docs => self.docs = folded,
        }
    }
}

/// Which of the workshop's views the area beside the sidebar shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

/// Which panel the strip along the canvas shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum PanelTab {
    #[default]
    Controls,
    Actions,
    Interactions,
    Layout,
    Accessibility,
    Snapshot,
}

type OpenGroups = RwSignal<HashSet<Arc<str>>>;

/// What the sidebar, the canvas and the panels share: the previews, which one is selected, which groups are open, the pane sizes, the view, the search, the panel shown and the actions filter, each preview's args, the canvas toolbar's settings, the matrix the Matrix view shows and its focused cell, the calls the mounted preview's callbacks received, what the inspection panels read of its canvas and what the last test run found of each snapshot.
///
/// A cheap handle: clones share one state. Everything but the previews, the remount count, the action log, the inspection, the test report and the search field is a `@workshop/…` hot signal, so it is made where the tree is built, after a hot reload has restored the values it carries.
#[derive(Clone)]
pub(crate) struct WorkshopState {
    entries: Rc<[PreviewEntry]>,
    selection: RwSignal<Option<Arc<str>>>,
    expanded: OpenGroups,
    rested: OpenGroups,
    view: RwSignal<ViewMode>,
    search: RwSignal<String>,
    sidebar_width: RwSignal<f32>,
    sidebar_collapsed: RwSignal<bool>,
    panel_size: RwSignal<f32>,
    panel_collapsed: RwSignal<bool>,
    panels_folded: RwSignal<FoldedViews>,
    panel_position: RwSignal<PanelPosition>,
    panel_tab: RwSignal<PanelTab>,
    actions_filter: RwSignal<String>,
    canvas_settings: RwSignal<CanvasSettings>,
    chrome: RwSignal<bool>,
    matrix_axes: RwSignal<MatrixAxes>,
    matrix_focus: RwSignal<Option<String>>,
    remounts: RwSignal<u64>,
    arrangement: Memo<Arrangement>,
    sidebar_drawer: RwSignal<bool>,
    actions: ActionLog,
    inspection: Inspection,
    interactions: Interactions,
    snapshots: SnapshotReview,
    search_field: Rc<Cell<Option<FocusId>>>,
    args: Rc<RefCell<HashMap<&'static str, Args>>>,
    /// The previews built once off screen so their args are known; see [`WorkshopState::learn_args`].
    learned: Rc<RefCell<HashSet<&'static str>>>,
    // Args are made lazily, often from inside a canvas, and would otherwise be freed with whichever canvas asked first.
    owner: OwnerId,
}

impl WorkshopState {
    /// [`Self::kept_in`] keeping nothing between runs and reading no test report: the state a test builds.
    #[cfg(test)]
    pub(crate) fn new(entries: Rc<[PreviewEntry]>, requested: &PreviewRequest) -> Self {
        Self::kept_in(entries, requested, &Keeper::none(), None)
    }

    /// The state for `entries`, with what a hot reload restored, and otherwise what `keeper` kept on the last run: the selection, the view, the sidebar groups left open, the pane sizes and the canvas toolbar's settings, each kept there again as it changes.
    ///
    /// The selection a hot reload restored wins while it still names a preview. Otherwise it is the preview `requested` names by id, else the stored one, else the first; where the requested component, read as a title filter, keeps any previews, the stored and the first are chosen among those. The search starts as that filter.
    ///
    /// The Snapshot panel reviews the `report.json` at `report`, read now.
    pub(crate) fn kept_in(
        entries: Rc<[PreviewEntry]>,
        requested: &PreviewRequest,
        keeper: &Keeper,
        report: Option<PathBuf>,
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
        let (rested, expanded) = Self::open_groups(&entries, selection.peek().as_deref(), keeper);
        let state = Self {
            selection,
            expanded,
            rested,
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
            panel_collapsed: signal(false),
            panels_folded: keeper.kept(
                keys::PANEL_COLLAPSED,
                store::keys::PANEL_COLLAPSED,
                FoldedViews::default(),
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
            matrix_axes: hot_signal(keys::MATRIX_AXES, MatrixAxes::default()),
            matrix_focus: hot_signal(keys::MATRIX_FOCUS, None),
            remounts: signal(0),
            arrangement: Arrangement::follow(),
            sidebar_drawer: signal(false),
            actions: ActionLog::new(),
            inspection: Inspection::new(),
            interactions: Interactions::new(keeper.kept(
                keys::PLAY_AUTO_RUN,
                store::keys::PLAY_AUTO_RUN,
                true,
            )),
            snapshots: SnapshotReview::reading(report),
            search_field: Rc::default(),
            args: Rc::default(),
            learned: Rc::default(),
            owner: owner_scope().id(),
            entries,
        };
        state.fold_per_view();
        state.rest_beside_search();
        state
    }

    /// The groups open outside a search and the groups the sidebar shows open, each as a hot reload restored it; otherwise the first as `keeper` kept it, else the groups holding `selected`, and the second as the first. Groups the previews no longer have are dropped from both.
    fn open_groups(
        entries: &[PreviewEntry],
        selected: Option<&str>,
        keeper: &Keeper,
    ) -> (OpenGroups, OpenGroups) {
        let holding: HashSet<Arc<str>> = selected
            .map(|id| groups_holding(entries, id).into_iter().collect())
            .unwrap_or_default();
        let rested = keeper.kept(keys::SIDEBAR_RESTED, store::keys::SIDEBAR_EXPANDED, holding);
        let expanded = hot_signal(keys::SIDEBAR_EXPANDED, rested.peek());
        let groups: HashSet<Arc<str>> = group_ids(entries).into_iter().collect();
        for open in [rested, expanded] {
            if !open.peek_with(|open| open.is_subset(&groups)) {
                open.update(|open| open.retain(|id| groups.contains(id)));
            }
        }
        (rested, expanded)
    }

    /// Keeps the groups open outside a search to what the sidebar shows while nothing is searched. A search opens groups for as long as it lasts: clearing it brings back the groups open before it, with those holding each preview selected during it.
    fn rest_beside_search(&self) {
        let (search, selection) = (self.search, self.selection);
        let (expanded, rested) = (self.expanded, self.rested);
        let entries = Rc::clone(&self.entries);
        let searching = move || search.peek_with(|query| !query.trim().is_empty());
        effect(move || {
            let open = expanded.get();
            if !searching() {
                rested.set_if_changed(open);
            }
        });
        effect(move || {
            let Some(id) = selection.get() else {
                return;
            };
            if !searching() {
                return;
            }
            let holding = groups_holding(&entries, &id);
            if !rested.peek_with(|open| holding.iter().all(|id| open.contains(id))) {
                rested.update(|open| open.extend(holding));
            }
        });
        let was_searching = Cell::new(searching());
        effect(move || {
            let now = search.with(|query| !query.trim().is_empty());
            if was_searching.replace(now) && !now {
                expanded.set(rested.peek());
            }
        });
    }

    /// Keeps [`panel_collapsed`](Self::panel_collapsed) to what the view shown keeps: a view brought up folds the panels as it last did, and folding them is kept for the view they were folded in.
    fn fold_per_view(&self) {
        let (view, collapsed, folded) = (self.view, self.panel_collapsed, self.panels_folded);
        collapsed.set(folded.peek().of(view.peek()));
        effect(move || {
            let shown = view.get();
            collapsed.set_if_changed(folded.peek().of(shown));
        });
        effect(move || {
            let now = collapsed.get();
            let shown = view.peek();
            if folded.peek().of(shown) != now {
                folded.update(|folded| folded.set(shown, now));
            }
        });
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

    /// The groups of the sidebar that are open, by the id the sidebar gives each. On a first run only the groups holding the selection are; a search opens the groups holding its matches for as long as it lasts.
    pub(crate) fn expanded(&self) -> RwSignal<HashSet<Arc<str>>> {
        self.expanded
    }

    pub(crate) fn view(&self) -> RwSignal<ViewMode> {
        self.view
    }

    pub(crate) fn search(&self) -> RwSignal<String> {
        self.search
    }

    /// Takes the keyboard to the search field, opening the sidebar first if it is collapsed or in its drawer.
    pub(crate) fn focus_search(&self) {
        match self.arrangement.get().sidebar_in_drawer() {
            true => self.sidebar_drawer.set(true),
            false => self.sidebar_collapsed.set(false),
        }
        if let Some(field) = self.search_field.get() {
            focus::request(field);
        }
    }

    /// Shows or hides the sidebar: its drawer while the window is narrow, its pane otherwise.
    pub(crate) fn toggle_sidebar(&self) {
        match self.arrangement.get().sidebar_in_drawer() {
            true => self.sidebar_drawer.toggle(),
            false => self.sidebar_collapsed.toggle(),
        }
    }

    /// How the chrome is arranged for the window's width now. Tracked.
    pub(crate) fn arrangement(&self) -> Memo<Arrangement> {
        self.arrangement
    }

    /// Whether the sidebar's drawer is open. Only a narrow window shows the sidebar in a drawer, and it starts shut there.
    pub(crate) fn sidebar_drawer(&self) -> RwSignal<bool> {
        self.sidebar_drawer
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

    /// Whether the panels are folded down to their tabs, as the view shown keeps them. Their size is kept for when they open again.
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

    /// What the Matrix view varies: each preview's own matrix, or the axes picked in its place for every preview alike.
    pub(crate) fn matrix_axes(&self) -> RwSignal<MatrixAxes> {
        self.matrix_axes
    }

    /// The id of the matrix cell the inspection panels and the marks follow; the view moves it to its first cell when it names none of the cells shown.
    pub(crate) fn matrix_focus(&self) -> RwSignal<Option<String>> {
        self.matrix_focus
    }

    /// Where the workshop is, as its address carries it: the page with the selected preview's edited args and, on the Matrix view, the axes of a matrix it builds and the focused cell. Subscribes the caller to all of it. `None` only when there are no previews.
    pub(crate) fn route(&self) -> Option<WorkshopRoute> {
        let entry = self.selected()?;
        let view = self.view.get();
        let mut route = route_of(&entry, view, self.chrome.get());
        route.args = edited_args(&self.args(&entry));
        if view == ViewMode::Matrix {
            route.axes = match self.matrix_axes.get() {
                MatrixAxes::Picked(picked) => Some(picked),
                MatrixAxes::Declared => None,
            };
            route.cell = self
                .matrix_focus
                .get()
                .filter(|cell| cell_past(entry.id, cell).is_some());
        }
        Some(route)
    }

    /// The page the workshop is on, and whether its chrome shows, without subscribing: where its address starts from, before [`Self::route`] adds what is set there.
    pub(crate) fn current_route(&self) -> Option<WorkshopRoute> {
        let id = self.selection.peek()?;
        let entry = self.entries.iter().find(|entry| *entry.id == *id)?;
        Some(route_of(entry, self.view.peek(), self.chrome.peek()))
    }

    /// Goes where `route` points and sets what it carries: the args of the preview it lands on, every other arg back at its default, the Matrix view's axes and focused cell, and the canvas toolbar's settings. What it leaves out is left as it is, as is what already matches it, so opening the address the workshop reports changes nothing.
    ///
    /// An id or a docs page no preview has leaves the selection as it was, and sets nothing on a preview; an arg the preview no longer has, or a value it no longer takes, is left at its default.
    pub(crate) fn open(&self, route: &WorkshopRoute) {
        batch(|| {
            let (view, listed) = match &route.page {
                Page::Preview(id) => (ViewMode::Canvas, self.select(id)),
                Page::Matrix(id) => (ViewMode::Matrix, self.select(id)),
                Page::Docs(key) => (ViewMode::Docs, self.select_docs(key)),
            };
            if listed {
                self.view.set_if_changed(view);
            }
            self.chrome.set_if_changed(route.chrome);
            if let Some(settings) = &route.settings {
                self.canvas_settings.set_if_changed(settings.clone());
            }
            let Some(entry) = listed.then(|| self.current_entry()).flatten() else {
                return;
            };
            if !route.args.is_empty() {
                self.set_args(&entry, &route.args);
            }
            if let Some(axes) = &route.axes {
                self.matrix_axes
                    .set_if_changed(MatrixAxes::Picked(axes.clone()));
            }
            if let Some(cell) = &route.cell {
                self.matrix_focus.set_if_changed(Some(cell.clone()));
            }
        });
    }

    /// Sets `entry`'s args to `args` and every other one to its default, unless they already are.
    fn set_args(&self, entry: &PreviewEntry, args: &[(String, ArgValue)]) {
        let held = self.args(entry);
        let mut wanted = args.to_vec();
        wanted.sort_by(|(a, _), (b, _)| a.cmp(b));
        if edited_args(&held) == wanted {
            return;
        }
        held.reset();
        for (name, value) in args {
            let _ = held.set(name, value.clone());
        }
    }

    /// A link to where the workshop is now that also carries the canvas toolbar's settings: what "Copy link" puts on the clipboard, which `cargo telar preview <link>` opens again.
    pub(crate) fn link(&self) -> Option<String> {
        let mut route = self.route()?;
        route.settings = Some(self.canvas_settings.peek());
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

    /// Selects the first preview of the component whose docs page `key` names, unless the selected one already is.
    fn select_docs(&self, key: &str) -> bool {
        let documented = |entry: &PreviewEntry| {
            docs_path(entry.package, entry.title)
                .into_iter()
                .eq(title_path(key))
        };
        if self.current_entry().is_some_and(|entry| documented(&entry)) {
            return true;
        }
        let first = self.entries.iter().find(|entry| documented(entry));
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

    /// What the Layout and Accessibility panels show of the mounted canvas, or of the focused matrix cell, and the marks they draw over it.
    pub(crate) fn inspection(&self) -> Inspection {
        self.inspection
    }

    /// The play the canvas runs: the steps of its last run, the one picked to show, and whether it runs as the canvas mounts.
    pub(crate) fn interactions(&self) -> &Interactions {
        &self.interactions
    }

    /// What the last test run found of each snapshot, as the Snapshot panel, the matrix cells and the sidebar show it.
    pub(crate) fn snapshots(&self) -> &SnapshotReview {
        &self.snapshots
    }

    /// The calls the mounted preview's callbacks received. One log for as long as the workshop lives, so a panel reading it outlives each canvas, and emptied as each canvas mounts.
    pub(crate) fn actions(&self) -> ActionLog {
        self.actions
    }

    /// Builds `entry` once off screen, the first time it is asked for, so the args it reads as it builds are known where nothing has mounted it yet, such as among a matrix's axes: an arg the preview reads rather than declares is known only once a build has read it.
    pub(crate) fn learn_args(&self, entry: &PreviewEntry) {
        if !self.learned.borrow_mut().insert(entry.id) {
            return;
        }
        let ctx =
            PreviewCtx::for_entry_with(entry, self.args(entry)).with_actions(ActionLog::new());
        let entry = *entry;
        let built = with_owner(Some(self.owner), || {
            SurfaceCanvas::new(Size::new(400.0, 300.0), move || {
                mount_preview(&entry, ctx, |_| nothing(), None)
            })
        });
        drop(built);
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

fn nothing() -> Result<Box<dyn LayoutItem>, LayoutError> {
    Ok(box_item(Container::new(LayoutStyle::new(), Vec::new())?))
}

fn route_of(entry: &PreviewEntry, view: ViewMode, chrome: bool) -> WorkshopRoute {
    let page = match view {
        ViewMode::Canvas => Page::Preview(entry.id.into()),
        ViewMode::Matrix => Page::Matrix(entry.id.into()),
        ViewMode::Docs => Page::Docs(docs_key(entry.package, entry.title).into()),
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
