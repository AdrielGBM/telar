//! The app's address as the runner holds it: the platform's [`LocationSource`], and the history both sides last agreed on.

use platform_core::{HistoryUpdate, Location, LocationFormat, LocationSource};

use crate::user_preferences::{store_history, stored_history};

pub(crate) struct LocationBinding {
    source: Box<dyn LocationSource>,
    // A target whose platform keeps no history between runs — a command line — has it kept in the preference store instead, and reopened from there when launched without a location.
    remember: bool,
    history: Vec<Location>,
    opened: bool,
}

impl LocationBinding {
    pub(crate) fn new(source: Box<dyn LocationSource>) -> Self {
        Self {
            source,
            remember: false,
            history: Vec::new(),
            opened: false,
        }
    }

    #[cfg(any(
        test,
        feature = "tui",
        all(
            feature = "desktop-bare",
            not(target_os = "android"),
            not(target_arch = "wasm32")
        )
    ))]
    pub(crate) fn remembered(source: Box<dyn LocationSource>) -> Self {
        Self {
            remember: true,
            ..Self::new(source)
        }
    }

    /// A desktop window's binding: [`remembered`](Self::remembered), except in a `cargo telar preview` run. Its address is the workshop's, which keeps its own place between runs, and remembering it under the application's name would reopen the application at a workshop address and the workshop at the application's.
    #[cfg(all(
        feature = "desktop-bare",
        not(target_os = "android"),
        not(target_arch = "wasm32")
    ))]
    pub(crate) fn for_window(source: Box<dyn LocationSource>) -> Self {
        match std::env::var_os(telar_project::protocol::PREVIEW_VAR) {
            Some(_) => Self::new(source),
            None => Self::remembered(source),
        }
    }

    /// The history with `location` opened on top, as a link opened into the running app opens it.
    #[cfg(any(
        test,
        all(
            feature = "dev",
            not(target_os = "android"),
            not(target_arch = "wasm32")
        )
    ))]
    pub(crate) fn linking(&self, location: Location) -> Vec<Location> {
        let mut history = self.history.clone();
        history.push(location);
        history
    }

    /// The history to hand the app before it builds: the platform's opening one the first time, and whatever it has moved to since on every rebuild after.
    pub(crate) fn open(&mut self) -> &[Location] {
        if !self.opened {
            self.opened = true;
            self.history = self.source.initial();
            if self.history.is_empty() && self.remember {
                self.history = restore();
            }
        }
        &self.history
    }

    /// Carries out a move the app made.
    pub(crate) fn apply(&mut self, update: &HistoryUpdate) {
        self.source.apply(update);
        self.follow(update.history.clone());
    }

    /// Records a history the platform moved to by itself.
    pub(crate) fn follow(&mut self, history: Vec<Location>) {
        self.history = history;
        if !self.remember {
            return;
        }
        let format = LocationFormat::root();
        let written: Vec<String> = self.history.iter().map(|l| format.format(l)).collect();
        if stored_history() != written {
            store_history(&written);
        }
    }
}

fn restore() -> Vec<Location> {
    let format = LocationFormat::root().with_locales(platform_core::location_locales());
    stored_history()
        .iter()
        .filter_map(|written| format.parse(written))
        .collect()
}

#[cfg(test)]
#[path = "location.test.rs"]
mod tests;
