//! The workshop's keyboard: the shortcuts it answers anywhere, the command palette that finds every preview and runs every command, and the `?` overlay listing the shortcuts.
//!
//! Its chords hold Alt, so they never reach a control inside a preview and never meet the devtools' Ctrl+Shift ones. None fires while a text entry has the caret, in the chrome or inside a canvas: the keys are the field's.

mod help;
mod palette;

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use telar::preview::PreviewEntry;
use telar::{
    Direction, Key, LayoutError, LayoutItem, ModifiersState, NamedKey, RwSignal, focus, signal,
};

use crate::canvas_toolbar::locales;
use crate::settings::Zoom;
use crate::sidebar::listed;
use crate::state::{PanelPosition, ViewMode, WorkshopState};
use crate::strings::{
    CYCLE_DIRECTION, CYCLE_LOCALE, CYCLE_MODE, FIND, FULLSCREEN, GROUP_CANVAS, GROUP_NAVIGATE,
    GROUP_VIEW, KEYBOARD_SHORTCUTS, MOVE_PANELS, NEXT_COMPONENT, NEXT_PREVIEW, PREVIOUS_COMPONENT,
    PREVIOUS_PREVIEW, REMOUNT, SEARCH, SHOW_CANVAS, SHOW_DOCS, TOGGLE_GRID, TOGGLE_PANELS,
    TOGGLE_RULERS, TOGGLE_SIDEBAR, ZOOM_IN, ZOOM_OUT, ZOOM_RESET,
};

/// Something the workshop does on a shortcut, from the palette or both.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    Find,
    Shortcuts,
    PreviousPreview,
    NextPreview,
    PreviousComponent,
    NextComponent,
    Search,
    ShowCanvas,
    ShowDocs,
    ToggleSidebar,
    TogglePanels,
    MovePanels,
    Fullscreen,
    CycleMode,
    CycleLocale,
    CycleDirection,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    ToggleGrid,
    ToggleRulers,
    Remount,
}

/// Every action, in the order the palette and the shortcuts overlay list them.
pub(crate) const ACTIONS: [Action; 22] = [
    Action::Find,
    Action::Shortcuts,
    Action::PreviousPreview,
    Action::NextPreview,
    Action::PreviousComponent,
    Action::NextComponent,
    Action::Search,
    Action::ShowCanvas,
    Action::ShowDocs,
    Action::ToggleSidebar,
    Action::TogglePanels,
    Action::MovePanels,
    Action::Fullscreen,
    Action::CycleMode,
    Action::CycleLocale,
    Action::CycleDirection,
    Action::ZoomIn,
    Action::ZoomOut,
    Action::ZoomReset,
    Action::ToggleGrid,
    Action::ToggleRulers,
    Action::Remount,
];

/// The groups the actions are listed under, as [`crate::strings`] keys, in order.
pub(crate) const GROUPS: [&str; 3] = [GROUP_NAVIGATE, GROUP_VIEW, GROUP_CANVAS];

impl Action {
    /// Unique among the palette's commands, and never a preview's.
    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::Find => "workshop:find",
            Self::Shortcuts => "workshop:shortcuts",
            Self::PreviousPreview => "workshop:previous-preview",
            Self::NextPreview => "workshop:next-preview",
            Self::PreviousComponent => "workshop:previous-component",
            Self::NextComponent => "workshop:next-component",
            Self::Search => "workshop:search",
            Self::ShowCanvas => "workshop:show-canvas",
            Self::ShowDocs => "workshop:show-docs",
            Self::ToggleSidebar => "workshop:toggle-sidebar",
            Self::TogglePanels => "workshop:toggle-panels",
            Self::MovePanels => "workshop:move-panels",
            Self::Fullscreen => "workshop:fullscreen",
            Self::CycleMode => "workshop:cycle-mode",
            Self::CycleLocale => "workshop:cycle-locale",
            Self::CycleDirection => "workshop:cycle-direction",
            Self::ZoomIn => "workshop:zoom-in",
            Self::ZoomOut => "workshop:zoom-out",
            Self::ZoomReset => "workshop:zoom-reset",
            Self::ToggleGrid => "workshop:toggle-grid",
            Self::ToggleRulers => "workshop:toggle-rulers",
            Self::Remount => "workshop:remount",
        }
    }

    /// What it is called, as a [`crate::strings`] key.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Find => FIND,
            Self::Shortcuts => KEYBOARD_SHORTCUTS,
            Self::PreviousPreview => PREVIOUS_PREVIEW,
            Self::NextPreview => NEXT_PREVIEW,
            Self::PreviousComponent => PREVIOUS_COMPONENT,
            Self::NextComponent => NEXT_COMPONENT,
            Self::Search => SEARCH,
            Self::ShowCanvas => SHOW_CANVAS,
            Self::ShowDocs => SHOW_DOCS,
            Self::ToggleSidebar => TOGGLE_SIDEBAR,
            Self::TogglePanels => TOGGLE_PANELS,
            Self::MovePanels => MOVE_PANELS,
            Self::Fullscreen => FULLSCREEN,
            Self::CycleMode => CYCLE_MODE,
            Self::CycleLocale => CYCLE_LOCALE,
            Self::CycleDirection => CYCLE_DIRECTION,
            Self::ZoomIn => ZOOM_IN,
            Self::ZoomOut => ZOOM_OUT,
            Self::ZoomReset => ZOOM_RESET,
            Self::ToggleGrid => TOGGLE_GRID,
            Self::ToggleRulers => TOGGLE_RULERS,
            Self::Remount => REMOUNT,
        }
    }

    /// One of [`GROUPS`].
    pub(crate) fn group(self) -> &'static str {
        match self {
            Self::Find
            | Self::Shortcuts
            | Self::PreviousPreview
            | Self::NextPreview
            | Self::PreviousComponent
            | Self::NextComponent
            | Self::Search => GROUP_NAVIGATE,
            Self::ShowCanvas
            | Self::ShowDocs
            | Self::ToggleSidebar
            | Self::TogglePanels
            | Self::MovePanels
            | Self::Fullscreen => GROUP_VIEW,
            Self::CycleMode
            | Self::CycleLocale
            | Self::CycleDirection
            | Self::ZoomIn
            | Self::ZoomOut
            | Self::ZoomReset
            | Self::ToggleGrid
            | Self::ToggleRulers
            | Self::Remount => GROUP_CANVAS,
        }
    }

    /// The chords that run it, written as `kbd` shows them; [`action_for`] is what answers them.
    pub(crate) fn chords(self) -> &'static [&'static str] {
        match self {
            Self::Find => &["/", "Mod+K"],
            Self::Shortcuts => &["?"],
            Self::PreviousPreview => &["Alt+↑"],
            Self::NextPreview => &["Alt+↓"],
            Self::PreviousComponent => &["Alt+←"],
            Self::NextComponent => &["Alt+→"],
            Self::Search => &[],
            Self::ShowCanvas => &["Alt+1"],
            Self::ShowDocs => &["Alt+3"],
            Self::ToggleSidebar => &["Alt+S"],
            Self::TogglePanels => &["Alt+A"],
            Self::MovePanels => &["Alt+D"],
            Self::Fullscreen => &["Alt+F"],
            Self::CycleMode => &["Alt+T"],
            Self::CycleLocale => &["Alt+L"],
            Self::CycleDirection => &["Alt+R"],
            Self::ZoomIn => &["Alt+="],
            Self::ZoomOut => &["Alt+-"],
            Self::ZoomReset => &["Alt+0"],
            Self::ToggleGrid => &["Alt+G"],
            Self::ToggleRulers => &["Alt+Shift+G"],
            Self::Remount => &["Alt+Shift+R"],
        }
    }
}

/// The action the press that reported `key` runs. With Alt held the layout may have composed `key` into another character, as Option does on macOS, so the chord is matched on the key the press makes with no modifier.
fn pressed_action(key: &Key, modifiers: ModifiersState) -> Option<Action> {
    if modifiers.is_alt {
        action_for(&telar::shortcut_key(key), modifiers)
    } else {
        action_for(key, modifiers)
    }
}

/// The action a key press runs. Letters are matched whatever their case, so Caps Lock changes nothing and Shift is read from `modifiers`.
pub(crate) fn action_for(key: &Key, modifiers: ModifiersState) -> Option<Action> {
    let command = modifiers.is_ctrl || modifiers.is_meta;
    match key {
        Key::Char(c) if command => {
            (!modifiers.is_alt && c.eq_ignore_ascii_case(&'k')).then_some(Action::Find)
        }
        _ if command => None,
        _ if modifiers.is_alt => alt_action(key, modifiers.is_shift),
        Key::Char('/') => Some(Action::Find),
        Key::Char('?') => Some(Action::Shortcuts),
        _ => None,
    }
}

fn alt_action(key: &Key, shift: bool) -> Option<Action> {
    let action = match key {
        Key::Named(NamedKey::ArrowUp) => Action::PreviousPreview,
        Key::Named(NamedKey::ArrowDown) => Action::NextPreview,
        Key::Named(NamedKey::ArrowLeft) => Action::PreviousComponent,
        Key::Named(NamedKey::ArrowRight) => Action::NextComponent,
        Key::Named(NamedKey::NumpadAdd) => Action::ZoomIn,
        Key::Named(NamedKey::NumpadSubtract) => Action::ZoomOut,
        Key::Char(c) => match (c.to_ascii_lowercase(), shift) {
            ('1', false) => Action::ShowCanvas,
            ('3', false) => Action::ShowDocs,
            ('s', false) => Action::ToggleSidebar,
            ('a', false) => Action::TogglePanels,
            ('d', false) => Action::MovePanels,
            ('f', false) => Action::Fullscreen,
            ('t', false) => Action::CycleMode,
            ('l', false) => Action::CycleLocale,
            ('r', false) => Action::CycleDirection,
            ('r', true) => Action::Remount,
            ('g', false) => Action::ToggleGrid,
            ('g', true) => Action::ToggleRulers,
            ('=' | '+', _) => Action::ZoomIn,
            ('-', false) => Action::ZoomOut,
            ('0', false) => Action::ZoomReset,
            _ => return None,
        },
        _ => return None,
    };
    Some(action)
}

/// The id a preview is listed under in the palette.
fn preview_command(entry: &PreviewEntry) -> Arc<str> {
    format!("preview:{}", entry.id).into()
}

/// What runs the workshop's actions, and the palette and shortcuts overlay it opens. A cheap handle: clones share one.
#[derive(Clone)]
pub(crate) struct Keymap {
    state: WorkshopState,
    palette: RwSignal<bool>,
    shortcuts: RwSignal<bool>,
    /// Whether the sidebar and the panels were folded before the canvas went fullscreen, to put them back as they were.
    before_fullscreen: Rc<Cell<Option<(bool, bool)>>>,
}

impl Keymap {
    pub(crate) fn new(state: &WorkshopState) -> Self {
        Self {
            state: state.clone(),
            palette: signal(false),
            shortcuts: signal(false),
            before_fullscreen: Rc::default(),
        }
    }

    /// The palette and the shortcuts overlay, each drawn over the workshop while open, for the shell to place.
    pub(crate) fn overlays(&self) -> Result<Vec<Box<dyn LayoutItem>>, LayoutError> {
        Ok(vec![palette::palette(self)?, help::shortcuts(self)?])
    }

    /// Runs the action `key` names, and answers whether there was one. Nothing runs while a text entry has the caret, nor while the palette is open; while the shortcuts overlay is, only `?` does, closing it.
    pub(crate) fn on_key(&self, key: &Key) -> bool {
        if focus::text_entry_focused() || self.palette.peek() {
            return false;
        }
        let Some(action) = pressed_action(key, telar::modifiers()) else {
            return false;
        };
        if self.shortcuts.peek() && action != Action::Shortcuts {
            return false;
        }
        self.run(action);
        true
    }

    pub(crate) fn run(&self, action: Action) {
        let state = &self.state;
        let settings = state.canvas_settings();
        match action {
            Action::Find => self.palette.set(true),
            Action::Shortcuts => self.shortcuts.update(|open| *open = !*open),
            Action::PreviousPreview => self.step_preview(-1),
            Action::NextPreview => self.step_preview(1),
            Action::PreviousComponent => self.step_component(-1),
            Action::NextComponent => self.step_component(1),
            Action::Search => state.focus_search(),
            Action::ShowCanvas => state.view().set(ViewMode::Canvas),
            Action::ShowDocs => state.view().set(ViewMode::Docs),
            Action::ToggleSidebar => state
                .sidebar_collapsed()
                .update(|folded| *folded = !*folded),
            Action::TogglePanels => state.panel_collapsed().update(|folded| *folded = !*folded),
            Action::MovePanels => state.panel_position().update(|position| {
                *position = match position {
                    PanelPosition::Bottom => PanelPosition::Right,
                    PanelPosition::Right => PanelPosition::Bottom,
                }
            }),
            Action::Fullscreen => self.toggle_fullscreen(),
            Action::CycleMode => {
                let modes = telar::registered_modes();
                settings.update(|settings| settings.mode = cycle(&modes, &settings.mode));
            }
            Action::CycleLocale => {
                let locales = locales(state);
                settings.update(|settings| settings.locale = cycle(&locales, &settings.locale));
            }
            Action::CycleDirection => settings.update(|settings| {
                settings.direction = cycle(&[Direction::Ltr, Direction::Rtl], &settings.direction)
            }),
            Action::ZoomIn => settings.update(|settings| settings.zoom = settings.zoom.zoomed_in()),
            Action::ZoomOut => {
                settings.update(|settings| settings.zoom = settings.zoom.zoomed_out())
            }
            Action::ZoomReset => settings.update(|settings| settings.zoom = Zoom::Fit),
            Action::ToggleGrid => settings.update(|settings| settings.grid = !settings.grid),
            Action::ToggleRulers => settings.update(|settings| settings.rulers = !settings.rulers),
            Action::Remount => state.remount(),
        }
    }

    /// Runs the palette command `id` names: selects a preview, or runs an action.
    fn run_command(&self, id: &str) {
        if let Some(entry) = self
            .state
            .entries()
            .iter()
            .find(|entry| *preview_command(entry) == *id)
        {
            self.state.select(entry.id);
        } else if let Some(action) = ACTIONS.into_iter().find(|action| action.id() == id) {
            self.run(action);
        }
    }

    /// The previews as the sidebar lists them now, narrowed by its search.
    fn listed(&self) -> Vec<PreviewEntry> {
        self.state
            .search()
            .peek_with(|query| listed(self.state.entries(), query))
    }

    /// Selects the preview `delta` rows from the selected one, staying at either end.
    fn step_preview(&self, delta: isize) {
        let listed = self.listed();
        let selected = self.state.selection().peek();
        let at = listed
            .iter()
            .position(|entry| selected.as_deref() == Some(entry.id));
        if let Some(entry) = stepped(&listed, at, delta) {
            self.state.select(entry.id);
        }
    }

    /// Selects the first preview of the component `delta` components from the selected one's, as the sidebar lists them.
    fn step_component(&self, delta: isize) {
        let mut firsts: Vec<PreviewEntry> = Vec::new();
        for entry in self.listed() {
            if !firsts.iter().any(|first| first.title == entry.title) {
                firsts.push(entry);
            }
        }
        let selected = self.state.selection().peek();
        let title = self
            .state
            .entries()
            .iter()
            .find(|entry| selected.as_deref() == Some(entry.id))
            .map(|entry| entry.title);
        let at = firsts.iter().position(|first| Some(first.title) == title);
        if let Some(entry) = stepped(&firsts, at, delta) {
            self.state.select(entry.id);
        }
    }

    /// Folds the sidebar and the panels away, or puts them back as they were.
    fn toggle_fullscreen(&self) {
        let (sidebar, panels) = (self.state.sidebar_collapsed(), self.state.panel_collapsed());
        let (shown_sidebar, shown_panels) = if sidebar.peek() && panels.peek() {
            self.before_fullscreen.take().unwrap_or((false, false))
        } else {
            self.before_fullscreen
                .set(Some((sidebar.peek(), panels.peek())));
            (true, true)
        };
        sidebar.set(shown_sidebar);
        panels.set(shown_panels);
    }
}

/// The item `delta` places from `at` in `items`, staying at either end; from no item, the first going forwards and the last going back.
fn stepped<T>(items: &[T], at: Option<usize>, delta: isize) -> Option<&T> {
    let last = items.len().checked_sub(1)?;
    let to = match at {
        Some(at) => at.saturating_add_signed(delta).min(last),
        None if delta > 0 => 0,
        None => last,
    };
    items.get(to)
}

/// The option after `current` in `options`, or none after the last, so cycling comes back to leaving the setting to the preview. A value `options` does not hold moves to the first.
fn cycle<T: Clone + PartialEq>(options: &[T], current: &Option<T>) -> Option<T> {
    match current
        .as_ref()
        .and_then(|current| options.iter().position(|option| option == current))
    {
        Some(at) => options.get(at + 1).cloned(),
        None => options.first().cloned(),
    }
}

#[cfg(test)]
#[path = "keymap_test.rs"]
mod tests;
