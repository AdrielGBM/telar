//! [`Navigator`]: the route stack, and the push/pop/replace commands that move it.

use std::rc::Rc;

use platform_core::{HistoryFollower, Location};
use reactive_core::{RwSignal, signal};

use crate::route::Route;

/// A reactive navigation stack over an app-defined route type `R` (typically a small `Clone + Eq` enum).
///
/// The stack is never empty: the root route stays at the bottom, so [`current`](Self::current) always yields a page and [`pop`](Self::pop) is a no-op at the root. Reads ([`current`](Self::current), [`depth`](Self::depth), [`can_pop`](Self::can_pop), [`with_stack`](Self::with_stack)) subscribe the caller, so a widget that renders `nav.current()` re-renders on every navigation. `Copy` — it is a handle to one reactive signal — and shared between the shell that reads it and the controls that push/pop it, exactly like the app-state signals threaded through a GUI.
pub struct Navigator<R: Clone + 'static> {
    stack: RwSignal<Vec<R>>,
}

impl<R: Clone + 'static> Clone for Navigator<R> {
    fn clone(&self) -> Self {
        *self
    }
}

// A handle to a signal and nothing more, so it moves into as many closures as a view has buttons.
impl<R: Clone + 'static> Copy for Navigator<R> {}

impl<R: Clone + 'static> Navigator<R> {
    /// Creates a navigator whose stack holds a single `root` page.
    pub fn new(root: R) -> Self {
        Self {
            stack: signal(vec![root]),
        }
    }

    /// Adopts an externally owned stack signal, seeding it with `root` when it is empty.
    ///
    /// Lets the stack come from somewhere the navigator itself cannot reach — notably `telar::hot_signal`, so the history survives a hot-reload dylib swap. The `root` seed also repairs a restored snapshot that deserialized to an empty vector, upholding the never-empty invariant [`current`](Self::current) relies on.
    pub fn from_signal(stack: RwSignal<Vec<R>>, root: R) -> Self {
        if stack.with(|s| s.is_empty()) {
            stack.update(|s| s.push(root));
        }
        Self { stack }
    }

    /// Pushes a page onto the stack, making it the new current page (adds a history entry).
    pub fn push(&self, route: R) {
        self.stack.update(|s| s.push(route));
    }

    /// Pops the current page, returning to the one beneath. No-op returning `false` when already at the root (the stack always keeps at least the root).
    pub fn pop(&self) -> bool {
        // Read the length under its own borrow first: `.with` holds the runtime borrow across the closure, so mutating inside it would re-borrow. Release it, then `.update`.
        if self.stack.with(|s| s.len()) <= 1 {
            return false;
        }
        self.stack.update(|s| {
            s.pop();
        });
        true
    }

    /// One "back" as the user means it: closes the frontmost open dialog or drawer if there is one, otherwise pops a page. Reports whether anything happened, so a caller wiring a hardware/gesture back can let the gesture fall through to the OS (exiting the app) when this returns `false` at the root with nothing open.
    ///
    /// Prefer this over [`pop`](Self::pop) for any general back affordance: popping directly would tear the page out from under an open dialog instead of closing the dialog the user is looking at.
    pub fn back(&self) -> bool {
        ui_core::dismiss::dismiss_top() || self.pop()
    }

    /// Pops every page above the root in one step.
    pub fn pop_to_root(&self) {
        if self.stack.with(|s| s.len()) > 1 {
            self.stack.update(|s| s.truncate(1));
        }
    }

    /// Replaces the current page in place, without adding a history entry.
    pub fn replace(&self, route: R) {
        self.stack.update(|s| {
            s.pop();
            s.push(route);
        });
    }

    /// Clears the whole stack down to a single `root` page.
    pub fn reset(&self, root: R) {
        self.stack.update(|s| {
            s.clear();
            s.push(root);
        });
    }

    /// Reactive read of the current (top) page.
    pub fn current(&self) -> R {
        self.stack
            .with(|s| s.last().expect("navigator stack is never empty").clone())
    }

    /// Reactive read of the stack depth (`1` at the root).
    pub fn depth(&self) -> usize {
        self.stack.with(|s| s.len())
    }

    /// Reactive read of whether there is a page to pop back to (`depth > 1`).
    pub fn can_pop(&self) -> bool {
        self.stack.with(|s| s.len() > 1)
    }

    /// Reactive read of the whole stack, root-first — for a breadcrumb or custom back logic.
    pub fn with_stack<T>(&self, f: impl FnOnce(&[R]) -> T) -> T {
        self.stack.with(|s| f(s))
    }

    /// Non-subscribing read of the whole stack, root-first.
    pub fn peek_stack<T>(&self, f: impl FnOnce(&[R]) -> T) -> T {
        self.stack.peek_with(|s| f(s))
    }

    /// The backing signal, for callers that need to observe or drive the stack directly.
    pub fn signal(&self) -> RwSignal<Vec<R>> {
        self.stack
    }
}

impl<R: Route + 'static> Navigator<R> {
    /// Reactive read of the current page's location — the address a platform's
    /// [`LocationSource`](platform_core::LocationSource) shows for it.
    pub fn location(&self) -> Location {
        self.current().to_location()
    }

    /// Reactive read of the whole stack as locations, root-first — what a target restores its native
    /// history from (a browser's session history, a desktop deep-link stack).
    pub fn locations(&self) -> Vec<Location> {
        self.with_stack(|s| s.iter().map(Route::to_location).collect())
    }

    /// Makes this stack the app's address, on every target: it opens on the location the app was launched or linked at, every push, pop and replace is reported to the platform's history, and the platform's own moves — a browser's back and forward, a link opened into the running app — become this stack.
    ///
    /// An entry this route type has no page for is left out rather than guessed at, and the platform's current entry is rewritten to what the stack shows instead, so a stale or hand-edited address settles on a real page without costing the user the entries before it. When nothing in the platform's history is recognised the stack stays as it was.
    ///
    /// One navigator follows the address at a time; a later call replaces an earlier one. The binding lasts as long as the reactive owner it was made under.
    pub fn follow_location(self) -> Self {
        let follower = Rc::new(Follower { nav: self });
        follower.adopt(&platform_core::location_history());
        let id = platform_core::follow_location_history(follower);
        let nav = self;
        reactive_core::effect(move || platform_core::report_location_history(nav.locations()));
        reactive_core::on_cleanup(move || platform_core::unfollow_location_history(id));
        self
    }
}

struct Follower<R: Route + 'static> {
    nav: Navigator<R>,
}

impl<R: Route + 'static> HistoryFollower for Follower<R> {
    fn adopt(&self, history: &[Location]) {
        let mut routes: Vec<R> = history.iter().filter_map(R::from_location).collect();
        if routes.is_empty() {
            routes = self.nav.peek_stack(<[R]>::to_vec);
        }
        let shown: Vec<Location> = routes.iter().map(Route::to_location).collect();
        // Before the stack moves, so the report its effect makes finds the platform already agreeing.
        if shown != history {
            platform_core::rewrite_location_history(shown.clone());
        }
        if self.nav.peek_stack(|stack| {
            stack
                .iter()
                .map(Route::to_location)
                .ne(shown.iter().cloned())
        }) {
            self.nav.stack.set(routes);
        }
    }

    fn push(&self, location: &Location) -> bool {
        R::from_location(location)
            .map(|route| self.nav.push(route))
            .is_some()
    }

    fn replace(&self, location: &Location) -> bool {
        R::from_location(location)
            .map(|route| self.nav.replace(route))
            .is_some()
    }

    fn back(&self) -> bool {
        self.nav.pop()
    }
}

#[cfg(test)]
#[path = "navigator_test.rs"]
mod tests;
