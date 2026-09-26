//! [`LocationSource`]: where a target keeps the app's address, and the history moves it is told about.

use std::sync::{Arc, Mutex};

use crate::{Location, LocationFormat};

/// One move of a history, as a platform history can take it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryStep {
    /// This many entries were added on top.
    Push(usize),
    /// The top entry was rewritten in place.
    Replace,
    /// This many entries were taken off the top.
    Back(usize),
}

/// A move of the app's history: the step, and the whole history after it, root-first.
///
/// The whole history travels with every step because the targets that keep one keep all of it: a browser stores it in each session-history entry so a reload or a traversal can restore the stack, and a desktop build remembers it between runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryUpdate {
    pub step: HistoryStep,
    pub history: Vec<Location>,
}

impl HistoryUpdate {
    /// The step that takes a platform showing `from` to `to`, or `None` when both are the same history.
    ///
    /// Growing on top is a push and shrinking from the top is a back. Anything else — a sibling replacing the top, a reset, a history the platform never had — rewrites the top entry, which is the one move that never leaves a platform entry pointing somewhere the app no longer is.
    pub fn between(from: &[Location], to: &[Location]) -> Option<Self> {
        if from == to {
            return None;
        }
        let step = if !from.is_empty() && to.len() > from.len() && to.starts_with(from) {
            HistoryStep::Push(to.len() - from.len())
        } else if !to.is_empty() && to.len() < from.len() && from.starts_with(to) {
            HistoryStep::Back(from.len() - to.len())
        } else {
            HistoryStep::Replace
        };
        Some(Self {
            step,
            history: to.to_vec(),
        })
    }
}

/// Where a target keeps the app's address: the location it opens on, and the history it is told about as the app moves.
///
/// Changes the platform makes on its own — a browser's back and forward, a link opened into the running app — arrive as [`Event::LocationChanged`](crate::Event::LocationChanged) rather than through this trait. Every method receives the whole history, root-first; its last entry is the current location.
pub trait LocationSource {
    /// The history the app opens on, root-first. Empty when the platform names no location, and the app starts at its own root.
    fn initial(&mut self) -> Vec<Location>;

    /// A new entry was added on top; `history` ends with it.
    fn push(&mut self, history: &[Location]);

    /// The top entry was rewritten in place; `history` ends with it.
    fn replace(&mut self, history: &[Location]);

    /// `count` entries were taken off the top; `history` is what remains.
    fn back(&mut self, count: usize, history: &[Location]);

    /// Carries out `update`, one entry at a time for a push of several.
    fn apply(&mut self, update: &HistoryUpdate) {
        let history = &update.history;
        match update.step {
            HistoryStep::Push(count) => {
                let first = history.len() - count.min(history.len());
                for end in first + 1..=history.len() {
                    self.push(&history[..end]);
                }
            }
            HistoryStep::Replace => self.replace(history),
            HistoryStep::Back(count) => self.back(count, history),
        }
    }
}

/// The location a process was started at, from a `--location <reference>` or `--location=<reference>` argument: the deep link of a desktop or terminal build.
///
/// A dedicated flag rather than the first free argument, so an application that takes file paths on its command line never has one read as an address. The reference is anything [`LocationFormat::parse`] reads, a URI included. A command line is read once, so the moves the app makes afterwards go nowhere.
#[derive(Clone, Debug, Default)]
pub struct ArgumentLocation {
    location: Option<Location>,
}

impl ArgumentLocation {
    pub const FLAG: &'static str = "--location";

    /// The location named in `args`. Scanning stops at `--`, after which nothing is a flag.
    pub fn from_args<I, S>(args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut args = args.into_iter();
        let mut reference = None;
        while let Some(arg) = args.next() {
            let arg = arg.as_ref();
            if arg == "--" {
                break;
            }
            if arg == Self::FLAG {
                reference = args.next().map(|value| value.as_ref().to_string());
                break;
            }
            if let Some(value) = arg
                .strip_prefix(Self::FLAG)
                .and_then(|rest| rest.strip_prefix('='))
            {
                reference = Some(value.to_string());
                break;
            }
        }
        Self {
            location: reference.and_then(|reference| LocationFormat::root().parse(&reference)),
        }
    }

    /// The location this process's own command line names.
    pub fn from_env() -> Self {
        Self::from_args(
            std::env::args_os()
                .skip(1)
                .filter_map(|arg| arg.into_string().ok()),
        )
    }

    pub fn location(&self) -> Option<&Location> {
        self.location.as_ref()
    }
}

impl LocationSource for ArgumentLocation {
    fn initial(&mut self) -> Vec<Location> {
        self.location.iter().cloned().collect()
    }

    fn push(&mut self, _history: &[Location]) {}

    fn replace(&mut self, _history: &[Location]) {}

    fn back(&mut self, _count: usize, _history: &[Location]) {}
}

/// Where a [`FixedLocation`] writes the history it was last told about.
pub type HistorySink = Arc<Mutex<Vec<Location>>>;

/// A history declared by whoever built the platform rather than chosen by a user: a headless run, a test, a prerender of one page. Moves are recorded, never acted on.
#[derive(Clone, Debug, Default)]
pub struct FixedLocation {
    history: Vec<Location>,
    sink: Option<HistorySink>,
}

impl FixedLocation {
    /// Opens on `history`, root-first.
    pub fn new(history: impl IntoIterator<Item = Location>) -> Self {
        Self {
            history: history.into_iter().collect(),
            sink: None,
        }
    }

    /// Writes the history into `sink` on open and after every move, for a caller that asserts on where the app went.
    pub fn recording_into(mut self, sink: HistorySink) -> Self {
        self.sink = Some(sink);
        self
    }

    fn record(&self, history: &[Location]) {
        if let Some(sink) = &self.sink {
            *sink.lock().unwrap() = history.to_vec();
        }
    }
}

impl LocationSource for FixedLocation {
    fn initial(&mut self) -> Vec<Location> {
        self.record(&self.history);
        self.history.clone()
    }

    fn push(&mut self, history: &[Location]) {
        self.record(history);
    }

    fn replace(&mut self, history: &[Location]) {
        self.record(history);
    }

    fn back(&mut self, _count: usize, history: &[Location]) {
        self.record(history);
    }
}

#[cfg(test)]
#[path = "location_source_test.rs"]
mod tests;
