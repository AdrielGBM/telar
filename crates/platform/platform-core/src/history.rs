//! The app's side of its address: the history the platform last reported, whoever follows it, and the moves reported back.
//!
//! One history per app. A browser tab, an activity and a command line each carry one address, so the store is per thread rather than per surface: a window opened beside the primary one has nothing of its own to report.
//!
//! A location's fragment names an anchor on a page, not a page. Following one adds an entry to the platform's history — back returns to where the reader was — but the follower only ever sees pages: an entry that names an anchor on the page before it is that same page.
//!
//! A location's locale names the language a page is shown in, not a page either. An app that binds one ([`bind_location_locale`]) has its whole history written in a single locale: the one its address was opened at, or the one the app switched to since. The follower never sees it.

use std::cell::RefCell;
use std::rc::Rc;

use crate::{HistoryStep, HistoryUpdate, Location, WindowCommand, push_window_command};

/// What keeps the app's pages in step with its address — in practice a route stack, and in practice `Navigator::follow_location`.
///
/// It sees pages only: every location it is handed or asked to open carries no fragment and no locale, and an anchor followed on a page is not an entry of its own here (see the module docs).
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

/// What keeps the app's language in step with the locale its address carries — in practice `telar::follow_location_locale`.
pub trait LocaleFollower {
    /// The locale to write an address in when the one the app opened at names none. One of the locales bound with it.
    fn choose(&self) -> String;

    /// The address now names `locale`: the one the app was opened or linked at, or the one a link went to. The app shows it.
    fn adopt(&self, locale: &str);
}

/// Names one [`follow_location_history`] registration, for [`unfollow_location_history`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HistoryFollowerId(u64);

type AnchorRevealer = Rc<dyn Fn(&str)>;

struct Locales {
    available: Vec<String>,
    follower: Rc<dyn LocaleFollower>,
    /// `None` until the history opens, when the address or the follower names one.
    current: Option<String>,
}

impl Locales {
    fn spelled(&self, locale: &str) -> Option<String> {
        self.available
            .iter()
            .find(|available| available.eq_ignore_ascii_case(locale))
            .cloned()
    }
}

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
    locales: Option<Locales>,
    /// Whether the platform has named the history the app opened on. A locale the app sets before then does not outrank the one the address names.
    opened: bool,
}

impl Hub {
    fn in_locale(&self, location: Location) -> Location {
        match self.locales.as_ref().and_then(|l| l.current.as_deref()) {
            Some(locale) => location.with_locale(locale),
            None => location,
        }
    }

    fn all_in_locale(&self, history: Vec<Location>) -> Vec<Location> {
        history
            .into_iter()
            .map(|location| self.in_locale(location))
            .collect()
    }

    /// Rewrites the whole history in the locale the address carries, answering with it when that changed anything.
    fn restamp(&mut self) -> Option<Vec<Location>> {
        let history = self.all_in_locale(self.history.clone());
        let changed = history != self.history;
        self.history = history.clone();
        changed.then_some(history)
    }
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

/// The page `location` shows: without the anchor it names and the locale it is shown in.
fn page_of(location: &Location) -> Location {
    location.clone().without_fragment().without_locale()
}

/// Whether `location` names an anchor on `page`, the page shown before it, rather than a page of its own.
fn folds_into(location: &Location, page: Option<&Location>) -> bool {
    location.fragment().is_some() && page == Some(&page_of(location))
}

/// The pages `history` shows, root-first: each entry without its anchor and its locale, and an entry that names an anchor on the page before it folded into that page.
pub fn pages_of(history: &[Location]) -> Vec<Location> {
    let mut pages: Vec<Location> = Vec::with_capacity(history.len());
    for location in history {
        if !folds_into(location, pages.last()) {
            pages.push(page_of(location));
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
            let page = page_of(location);
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

/// The app's history, root-first, as the platform last reported it or was last told — anchor entries included, every entry in the locale the address carries. Empty until a platform names one.
pub fn location_history() -> Vec<Location> {
    with_hub(|hub| hub.history.clone())
}

/// Hands over the history the platform reports — the one it opened on, or one it moved to by itself — and tells the follower the pages it shows. Arriving at an entry that names an anchor reveals it. The runner calls this; a test or a tool with no platform can call it to stand in for one.
///
/// With a locale bound, the first history settles the app's locale: the one its top entry names, or the follower's choice when it names none. Every later one is shown in the locale the app is in, and an entry that names another — one a browser returns to from before a switch — is rewritten in place.
pub fn receive_location_history(history: Vec<Location>) {
    let opening = with_hub(|hub| !std::mem::replace(&mut hub.opened, true));
    if opening {
        settle_locale(history.last().and_then(Location::locale));
    }
    let (shown, arrived_at) = with_hub(|hub| {
        let shown = hub.all_in_locale(history.clone());
        let before = hub.history.last().cloned();
        hub.history = shown.clone();
        hub.arriving = None;
        let arrived_at = shown
            .last()
            .filter(|top| before.as_ref() != Some(*top))
            .and_then(|top| top.fragment().map(str::to_owned));
        (shown, arrived_at)
    });
    if shown != history {
        replace_on_platform(shown.clone());
    }
    if let Some(follower) = follower() {
        follower.adopt(&pages_of(&shown));
    }
    if let Some(anchor) = arrived_at {
        request_reveal(anchor);
    }
}

/// The follower's pages are now `pages`; the platform is told the step that gets its history there, if there is one. The anchor entries of the pages that stay are kept.
pub fn report_location_history(pages: Vec<Location>) {
    let (history, arrived) = with_hub(|hub| {
        let mut history = hub.all_in_locale(history_showing(&hub.history, &pages));
        let arrived = hub
            .arriving
            .take_if(|anchor| history.last().map(page_of) == Some(page_of(anchor)))
            .map(|anchor| hub.in_locale(anchor));
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

fn replace_on_platform(history: Vec<Location>) {
    push_window_command(WindowCommand::Navigate(HistoryUpdate {
        step: HistoryStep::Replace,
        history,
    }));
}

/// The follower could not show the pages the platform reported as they were — an entry it has no page for, an address it writes differently — and shows `pages` instead. The platform's current entry is rewritten to match, never stepped back: the entries a user came from stay where they were.
pub fn rewrite_location_history(pages: Vec<Location>) {
    let changed = with_hub(|hub| {
        let history = hub.all_in_locale(history_showing(&hub.history, &pages));
        let changed = hub.history != history;
        hub.history = history.clone();
        changed.then_some(history)
    });
    if let Some(history) = changed {
        replace_on_platform(history);
    }
}

/// Opens `location` as a new entry: through the follower, which opens the page for it, or straight onto the platform's history when nothing follows it. `false` when the follower has no page for it.
///
/// A location that names an anchor opens its page if that is not the one shown, then adds the anchor as an entry of its own and reveals it once it is there.
///
/// A location that names one of the bound locales switches the app to it first, rewriting the entry shown rather than adding one; a link to the place being shown, in another language, goes no further than that.
pub fn push_location(location: Location) -> bool {
    let Some(location) = arrive_in_locale(location) else {
        return true;
    };
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
    let Some(location) = arrive_in_locale(location) else {
        return true;
    };
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

/// Switches to the locale `location` names, and answers with the place it names in no locale; `None` when the switch was the whole of the move, because that place is the one shown.
fn arrive_in_locale(location: Location) -> Option<Location> {
    let Some(locale) = location.locale().map(str::to_owned) else {
        return Some(location);
    };
    let place = location.without_locale();
    if !switch_locale(&locale, true) {
        return Some(place);
    }
    let shown = location_history().pop().map(Location::without_locale);
    (shown.as_ref() != Some(&place)).then_some(place)
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
    let entry = with_hub(|hub| hub.in_locale(page.with_fragment(name)));
    let mut history = location_history();
    if history.last() != Some(&entry) {
        history.push(entry);
        report_history(history);
    }
    request_reveal(name.to_owned());
}

/// The anchor `from` is now called `to` — a name that follows the locale, after a language switch — so every entry naming it names it by its new name, and the platform's entry is rewritten to match. What `anchor:` reports when the name it reads changes.
pub fn rename_anchor(from: &str, to: &str) {
    if from == to {
        return;
    }
    let renamed = |location: Location| match location.fragment() {
        Some(fragment) if fragment == from => location.with_fragment(to),
        _ => location,
    };
    let changed = with_hub(|hub| {
        if hub.unrevealed.as_deref() == Some(from) {
            hub.unrevealed = Some(to.to_owned());
        }
        hub.arriving = hub.arriving.take().map(renamed);
        let history: Vec<Location> = hub.history.iter().cloned().map(renamed).collect();
        let changed = hub.history != history;
        hub.history = history.clone();
        changed.then_some(history)
    });
    if let Some(history) = changed {
        replace_on_platform(history);
    }
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

/// Makes the app's address carry its locale, as one of `available`, with `follower` keeping the app's language in step with it. Replaces any binding before it; an app that never binds one has addresses with no locale in them.
///
/// Bound before the history opens — the usual place is the app's setup — the address the app opens at decides the locale. Bound after, the history it already has does, and is rewritten in that locale.
pub fn bind_location_locale(available: Vec<String>, follower: Rc<dyn LocaleFollower>) {
    let opened = with_hub(|hub| {
        let kept = hub.locales.take().and_then(|locales| locales.current);
        let mut locales = Locales {
            available,
            follower,
            current: None,
        };
        locales.current = kept.and_then(|locale| locales.spelled(&locale));
        hub.locales = Some(locales);
        hub.opened
    });
    if !opened {
        return;
    }
    if location_locale().is_none() {
        let named = location_history()
            .last()
            .and_then(|top| top.locale().map(str::to_owned));
        settle_locale(named.as_deref());
    }
    if let Some(history) = with_hub(Hub::restamp) {
        replace_on_platform(history);
    }
}

/// The locales the app's addresses carry, as [`bind_location_locale`] declared them. Empty for an app whose addresses carry none.
pub fn location_locales() -> Vec<String> {
    with_hub(|hub| {
        hub.locales
            .as_ref()
            .map(|locales| locales.available.clone())
            .unwrap_or_default()
    })
}

/// The locale the app's address is written in, once the history it opened on settled one.
pub fn location_locale() -> Option<String> {
    with_hub(|hub| hub.locales.as_ref().and_then(|l| l.current.clone()))
}

/// The app is now shown in `locale`: its address is rewritten in it, keeping the page and the anchor, in place of the entry shown rather than as a new one. Nothing happens for a locale the address does not carry, or before the history opens, when the address the app was opened at decides.
pub fn set_location_locale(locale: &str) {
    switch_locale(locale, false);
}

/// `location` as the app's address writes it: in the locale the address carries, unless it names one of its own.
pub(crate) fn in_location_locale(location: Location) -> Location {
    if location.locale().is_some() {
        return location;
    }
    with_hub(|hub| hub.in_locale(location))
}

/// Settles the locale the history opens in: `named` when it is one of the bound locales, the follower's choice otherwise. The follower is told either way.
fn settle_locale(named: Option<&str>) {
    let Some((follower, named)) = with_hub(|hub| {
        let locales = hub.locales.as_ref()?;
        Some((
            locales.follower.clone(),
            named.and_then(|locale| locales.spelled(locale)),
        ))
    }) else {
        return;
    };
    let chosen = named.unwrap_or_else(|| follower.choose());
    let settled = with_hub(|hub| {
        let locales = hub.locales.as_mut()?;
        let locale = locales
            .spelled(&chosen)
            .or_else(|| locales.available.first().cloned())?;
        locales.current = Some(locale.clone());
        Some(locale)
    });
    if let Some(locale) = settled {
        follower.adopt(&locale);
    }
}

/// Writes the whole history in `locale` and tells the platform, and the follower too when `adopt`. `false` when the address does not carry `locale`: none is bound, it is not one of them, or the history has not opened.
fn switch_locale(locale: &str, adopt: bool) -> bool {
    let Some((switched, changed)) = with_hub(|hub| {
        if !hub.opened {
            return None;
        }
        let locales = hub.locales.as_mut()?;
        let locale = locales.spelled(locale)?;
        if locales.current.as_deref() == Some(locale.as_str()) {
            return Some((None, None));
        }
        locales.current = Some(locale.clone());
        let follower = locales.follower.clone();
        Some((Some((follower, locale)), hub.restamp()))
    }) else {
        return false;
    };
    if let Some(history) = changed {
        replace_on_platform(history);
    }
    if adopt && let Some((follower, locale)) = switched {
        follower.adopt(&locale);
    }
    true
}

#[cfg(test)]
#[path = "history_test.rs"]
mod tests;
