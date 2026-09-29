//! The app's side of its address: the history the platform last reported, whoever follows it, and the moves reported back.
//!
//! One history per app. A browser tab, an activity and a command line each carry one address, so the store is per thread rather than per surface: a window opened beside the primary one has nothing of its own to report.
//!
//! A location's fragment names an anchor on a page, not a page. Following one adds an entry to the platform's history — back returns to where the reader was — but the follower only ever sees pages: an entry that names an anchor on the page before it is that same page.

use std::cell::RefCell;
use std::rc::Rc;

use crate::{HistoryUpdate, Location, WindowCommand, push_window_command};

/// What keeps the app's pages in step with its address — in practice a route stack, and in practice `Navigator::follow_location`.
///
/// It sees pages only: every location it is handed or asked to open carries no fragment, and an anchor followed on a page is not an entry of its own here (see the module docs).
pub trait HistoryFollower {
    /// The platform's pages are now `history`: the address the app was opened at, a back or forward, a link opened into the running app.
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

type AnchorRevealer = Rc<dyn Fn(&str)>;

#[derive(Default)]
struct Hub {
    history: Vec<Location>,
    follower: Option<(HistoryFollowerId, Rc<dyn HistoryFollower>)>,
    next_id: u64,
    revealer: Option<AnchorRevealer>,
    /// An anchor asked for before anything could reveal it: the fragment the app was opened at.
    unrevealed: Option<String>,
    /// An anchor to add on top once the follower reports the page it is on, for a push that names another page's anchor.
    arriving: Option<Location>,
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

/// Whether `location` names an anchor on `page`, the page shown before it, rather than a page of its own.
fn folds_into(location: &Location, page: Option<&Location>) -> bool {
    location.fragment().is_some() && page == Some(&location.clone().without_fragment())
}

/// The pages `history` shows, root-first: each entry without its anchor, and an entry that names an anchor on the page before it folded into that page.
pub fn pages_of(history: &[Location]) -> Vec<Location> {
    let mut pages: Vec<Location> = Vec::with_capacity(history.len());
    for location in history {
        if !folds_into(location, pages.last()) {
            pages.push(location.clone().without_fragment());
        }
    }
    pages
}

/// The whole history that shows `pages`: the entries of `current` for every page the two share from the root, anchors included, then the rest of `pages`.
fn history_showing(current: &[Location], pages: &[Location]) -> Vec<Location> {
    let mut kept = Vec::with_capacity(current.len().max(pages.len()));
    let mut shown: Vec<Location> = Vec::new();
    for location in current {
        if !folds_into(location, shown.last()) {
            let page = location.clone().without_fragment();
            if pages.get(shown.len()) != Some(&page) {
                break;
            }
            shown.push(page);
        }
        kept.push(location.clone());
    }
    kept.extend(pages[shown.len()..].iter().cloned());
    kept
}

/// The app's history, root-first, as the platform last reported it or was last told — anchor entries included. Empty until a platform names one.
pub fn location_history() -> Vec<Location> {
    with_hub(|hub| hub.history.clone())
}

/// Hands over the history the platform reports — the one it opened on, or one it moved to by itself — and tells the follower the pages it shows. Arriving at an entry that names an anchor reveals it. The runner calls this; a test or a tool with no platform can call it to stand in for one.
pub fn receive_location_history(history: Vec<Location>) {
    let arrived_at = with_hub(|hub| {
        let before = hub.history.last().cloned();
        hub.history = history.clone();
        hub.arriving = None;
        history
            .last()
            .filter(|top| before.as_ref() != Some(*top))
            .and_then(|top| top.fragment().map(str::to_owned))
    });
    if let Some(follower) = follower() {
        follower.adopt(&pages_of(&history));
    }
    if let Some(anchor) = arrived_at {
        request_reveal(anchor);
    }
}

/// The follower's pages are now `pages`; the platform is told the step that gets its history there, if there is one. The anchor entries of the pages that stay are kept.
pub fn report_location_history(pages: Vec<Location>) {
    let (history, arrived) = with_hub(|hub| {
        let mut history = history_showing(&hub.history, &pages);
        let arrived = hub
            .arriving
            .take_if(|anchor| history.last() == Some(&anchor.clone().without_fragment()));
        history.extend(arrived.clone());
        (history, arrived)
    });
    report_history(history);
    if let Some(fragment) = arrived.as_ref().and_then(Location::fragment) {
        request_reveal(fragment.to_owned());
    }
}

fn report_history(history: Vec<Location>) {
    let update = with_hub(|hub| {
        let update = HistoryUpdate::between(&hub.history, &history)?;
        hub.history = history;
        Some(update)
    });
    if let Some(update) = update {
        push_window_command(WindowCommand::Navigate(update));
    }
}

/// The follower could not show the pages the platform reported as they were — an entry it has no page for, an address it writes differently — and shows `pages` instead. The platform's current entry is rewritten to match, never stepped back: the entries a user came from stay where they were.
pub fn rewrite_location_history(pages: Vec<Location>) {
    let changed = with_hub(|hub| {
        let history = history_showing(&hub.history, &pages);
        let changed = hub.history != history;
        hub.history = history.clone();
        changed.then_some(history)
    });
    if let Some(history) = changed {
        push_window_command(WindowCommand::Navigate(HistoryUpdate {
            step: crate::HistoryStep::Replace,
            history,
        }));
    }
}

/// Opens `location` as a new entry: through the follower, which opens the page for it, or straight onto the platform's history when nothing follows it. `false` when the follower has no page for it.
///
/// A location that names an anchor opens its page if that is not the one shown, then adds the anchor as an entry of its own and reveals it once it is there.
pub fn push_location(location: Location) -> bool {
    let Some(anchor) = location.fragment().map(str::to_owned) else {
        return push_page(location);
    };
    let page = location.clone().without_fragment();
    if current_page().as_ref() == Some(&page) {
        push_anchor(&anchor);
        return true;
    }
    with_hub(|hub| hub.arriving = Some(location));
    let pushed = push_page(page);
    if !pushed {
        with_hub(|hub| hub.arriving = None);
    }
    pushed
}

fn push_page(location: Location) -> bool {
    if let Some(follower) = follower() {
        return follower.push(&location);
    }
    report_location_history(pages_with(|pages| pages.push(location)));
    true
}

/// Shows `location` in place of the current entry, the same way [`push_location`] opens one.
pub fn replace_location(location: Location) -> bool {
    let location = location.without_fragment();
    if let Some(follower) = follower() {
        return follower.replace(&location);
    }
    report_location_history(pages_with(|pages| {
        pages.pop();
        pages.push(location);
    }));
    true
}

/// Steps the app's history one entry back. `false` at its first entry, where going back is the platform's to decide.
///
/// An entry that names an anchor is stepped back over like any other, leaving the page where it is.
pub fn history_back() -> bool {
    let history = location_history();
    if history.len() >= 2 && history.last().and_then(Location::fragment).is_some() {
        let mut back = history;
        back.pop();
        let returned_to = back.last().and_then(Location::fragment).map(str::to_owned);
        report_history(back);
        if let Some(anchor) = returned_to {
            request_reveal(anchor);
        }
        return true;
    }
    if let Some(follower) = follower() {
        return follower.back();
    }
    let mut pages = pages_of(&history);
    if pages.len() < 2 {
        return false;
    }
    pages.pop();
    report_location_history(pages);
    true
}

fn pages_with(change: impl FnOnce(&mut Vec<Location>)) -> Vec<Location> {
    let mut pages = pages_of(&location_history());
    change(&mut pages);
    pages
}

fn current_page() -> Option<Location> {
    pages_of(&location_history()).pop()
}

/// Adds an entry naming the anchor `name` on the page being shown, and reveals it. Following the same anchor twice adds it once.
///
/// What following a link to an anchor does: the reader's place before it becomes an entry back returns to, and the address names where they went, as a same-page link does on the web.
pub fn push_anchor(name: &str) {
    let page = current_page().unwrap_or_default();
    let entry = page.with_fragment(name);
    let mut history = location_history();
    if history.last() != Some(&entry) {
        history.push(entry);
        report_history(history);
    }
    request_reveal(name.to_owned());
}

/// Makes `revealer` what reveals an anchor the history arrives at, and hands it the one the app was opened at if nothing could reveal that yet. Installed by the tree's anchor registry.
pub fn set_anchor_revealer(revealer: impl Fn(&str) + 'static) {
    let pending = with_hub(|hub| {
        hub.revealer = Some(Rc::new(revealer));
        hub.unrevealed.take()
    });
    if let Some(anchor) = pending {
        request_reveal(anchor);
    }
}

fn request_reveal(anchor: String) {
    let revealer = with_hub(|hub| {
        let revealer = hub.revealer.clone();
        if revealer.is_none() {
            hub.unrevealed = Some(anchor.clone());
        }
        revealer
    });
    if let Some(reveal) = revealer {
        reveal(&anchor);
    }
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
