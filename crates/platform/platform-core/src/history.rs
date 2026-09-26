//! The app's side of its address: the history the platform last reported, whoever follows it, and the moves reported back.
//!
//! One history per app. A browser tab, an activity and a command line each carry one address, so the store is per thread rather than per surface: a window opened beside the primary one has nothing of its own to report.

use std::cell::RefCell;
use std::rc::Rc;

use crate::{HistoryUpdate, Location, WindowCommand, push_window_command};

/// What keeps the app's pages in step with its address — in practice a route stack, and in practice `Navigator::follow_location`.
pub trait HistoryFollower {
    /// The platform's history is now `history`: the address the app was opened at, a back or forward, a link opened into the running app.
    fn adopt(&self, history: &[Location]);

    /// Opens `location` as a new entry. `false` when there is no page for it.
    fn push(&self, location: &Location) -> bool;

    /// Shows `location` in place of the current entry. `false` when there is no page for it.
    fn replace(&self, location: &Location) -> bool;

    /// Steps one entry back. `false` at the first one.
    fn back(&self) -> bool;
}

/// Names one [`follow_location_history`] registration, for [`unfollow_location_history`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HistoryFollowerId(u64);

#[derive(Default)]
struct Hub {
    history: Vec<Location>,
    follower: Option<(HistoryFollowerId, Rc<dyn HistoryFollower>)>,
    next_id: u64,
}

thread_local! {
    // Leaked rather than owned: a value with a destructor registers a TLS destructor, and one registered from a hot-reloaded library runs after that library is unmapped.
    static HUB: &'static RefCell<Hub> = Box::leak(Box::default());
}

fn with_hub<R>(f: impl FnOnce(&mut Hub) -> R) -> R {
    HUB.with(|hub| f(&mut hub.borrow_mut()))
}

fn follower() -> Option<Rc<dyn HistoryFollower>> {
    with_hub(|hub| hub.follower.as_ref().map(|(_, follower)| follower.clone()))
}

/// The app's history, root-first, as the platform last reported it or was last told. Empty until a platform names one.
pub fn location_history() -> Vec<Location> {
    with_hub(|hub| hub.history.clone())
}

/// Hands over the history the platform reports — the one it opened on, or one it moved to by itself — and tells the follower. The runner calls this; a test or a tool with no platform can call it to stand in for one.
pub fn receive_location_history(history: Vec<Location>) {
    with_hub(|hub| hub.history = history.clone());
    if let Some(follower) = follower() {
        follower.adopt(&history);
    }
}

/// The follower's history is now `history`; the platform is told the step that gets it there, if there is one.
pub fn report_location_history(history: Vec<Location>) {
    let update = with_hub(|hub| {
        let update = HistoryUpdate::between(&hub.history, &history)?;
        hub.history = history;
        Some(update)
    });
    if let Some(update) = update {
        push_window_command(WindowCommand::Navigate(update));
    }
}

/// The follower could not show the history the platform reported as it was — an entry it has no page for, an address it writes differently — and shows `history` instead. The platform's current entry is rewritten to match, never stepped back: the entries a user came from stay where they were.
pub fn rewrite_location_history(history: Vec<Location>) {
    let changed = with_hub(|hub| {
        let changed = hub.history != history;
        hub.history = history.clone();
        changed
    });
    if changed {
        push_window_command(WindowCommand::Navigate(HistoryUpdate {
            step: crate::HistoryStep::Replace,
            history,
        }));
    }
}

/// Opens `location` as a new entry: through the follower, which opens the page for it, or straight onto the platform's history when nothing follows it. `false` when the follower has no page for it.
pub fn push_location(location: Location) -> bool {
    if let Some(follower) = follower() {
        return follower.push(&location);
    }
    let mut history = location_history();
    history.push(location);
    report_location_history(history);
    true
}

/// Shows `location` in place of the current entry, the same way [`push_location`] opens one.
pub fn replace_location(location: Location) -> bool {
    if let Some(follower) = follower() {
        return follower.replace(&location);
    }
    let mut history = location_history();
    history.pop();
    history.push(location);
    report_location_history(history);
    true
}

/// Steps the app's history one entry back. `false` at its first entry, where going back is the platform's to decide.
pub fn history_back() -> bool {
    if let Some(follower) = follower() {
        return follower.back();
    }
    let mut history = location_history();
    if history.len() < 2 {
        return false;
    }
    history.pop();
    report_location_history(history);
    true
}

/// Makes `follower` the one the app's history is kept in step with, replacing any before it.
pub fn follow_location_history(follower: Rc<dyn HistoryFollower>) -> HistoryFollowerId {
    with_hub(|hub| {
        hub.next_id += 1;
        let id = HistoryFollowerId(hub.next_id);
        hub.follower = Some((id, follower));
        id
    })
}

/// Stops following, if `id` is still the follower: one that was replaced since has nothing left to withdraw.
pub fn unfollow_location_history(id: HistoryFollowerId) {
    with_hub(|hub| {
        if hub
            .follower
            .as_ref()
            .is_some_and(|(current, _)| *current == id)
        {
            hub.follower = None;
        }
    });
}

#[cfg(test)]
#[path = "history_test.rs"]
mod tests;
