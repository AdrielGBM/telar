//! The app's address as the runner holds it: the platform's [`LocationSource`], and the history both sides last agreed on.

use platform_core::{HistoryUpdate, Location, LocationFormat, LocationSource};

use crate::prefs::UserPrefs;

pub(crate) struct LocationBinding {
    source: Box<dyn LocationSource>,
    // A target whose platform keeps no history between runs — a command line — has it kept in `UserPrefs` instead, and reopened from there when launched without a location.
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

    /// The history to hand the app before it builds: the platform's opening one the first time, and whatever it has moved to since on every rebuild after.
    pub(crate) fn open(&mut self, prefs: &UserPrefs) -> &[Location] {
        if !self.opened {
            self.opened = true;
            self.history = self.source.initial();
            if self.history.is_empty() && self.remember {
                self.history = restore(prefs);
            }
        }
        &self.history
    }

    /// Carries out a move the app made. `true` when `prefs` changed and should be saved.
    pub(crate) fn apply(&mut self, update: &HistoryUpdate, prefs: &mut UserPrefs) -> bool {
        self.source.apply(update);
        self.follow(update.history.clone(), prefs)
    }

    /// Records a history the platform moved to by itself. `true` when `prefs` changed and should be saved.
    pub(crate) fn follow(&mut self, history: Vec<Location>, prefs: &mut UserPrefs) -> bool {
        self.history = history;
        if !self.remember {
            return false;
        }
        let format = LocationFormat::root();
        let written: Vec<String> = self.history.iter().map(|l| format.format(l)).collect();
        if prefs.history == written {
            return false;
        }
        prefs.history = written;
        true
    }
}

fn restore(prefs: &UserPrefs) -> Vec<Location> {
    let format = LocationFormat::root();
    prefs
        .history
        .iter()
        .filter_map(|written| format.parse(written))
        .collect()
}

#[cfg(test)]
#[path = "location_test.rs"]
mod tests;
