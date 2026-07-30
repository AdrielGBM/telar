use reactive_core::{RwSignal, signal};

/// A reactive navigation stack over an app-defined route type `R` (typically a small `Clone + Eq` enum).
///
/// The stack is never empty: the root route stays at the bottom, so [`current`](Self::current) always yields
/// a page and [`pop`](Self::pop) is a no-op at the root. Reads ([`current`](Self::current),
/// [`depth`](Self::depth), [`can_pop`](Self::can_pop), [`with_stack`](Self::with_stack)) subscribe the caller,
/// so a widget that renders `nav.current()` re-renders on every navigation. Cheap to clone — the inner
/// `RwSignal` is refcounted — and shared between the shell that reads it and the controls that push/pop it,
/// exactly like the app-state signals threaded through a GUI.
pub struct Navigator<R: Clone + 'static> {
    stack: RwSignal<Vec<R>>,
}

impl<R: Clone + 'static> Clone for Navigator<R> {
    fn clone(&self) -> Self {
        Self {
            stack: self.stack.clone(),
        }
    }
}

impl<R: Clone + 'static> Navigator<R> {
    /// Creates a navigator whose stack holds a single `root` page.
    pub fn new(root: R) -> Self {
        Self {
            stack: signal(vec![root]),
        }
    }

    /// Adopts an externally owned stack signal, seeding it with `root` when it is empty.
    ///
    /// Lets the stack come from somewhere the navigator itself cannot reach — notably `telar::hot_signal`, so
    /// the history survives a hot-reload dylib swap. The `root` seed also repairs a restored snapshot that
    /// deserialized to an empty vector, upholding the never-empty invariant [`current`](Self::current) relies on.
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

    /// Pops the current page, returning to the one beneath. No-op returning `false` when already at the root
    /// (the stack always keeps at least the root).
    pub fn pop(&self) -> bool {
        // Read the length under its own borrow first: `.with` holds the runtime borrow across the closure, so
        // mutating inside it would re-borrow. Release it, then `.update`.
        if self.stack.with(|s| s.len()) <= 1 {
            return false;
        }
        self.stack.update(|s| {
            s.pop();
        });
        true
    }

    /// One "back" as the user means it: closes the frontmost open dialog or drawer if there is one, otherwise
    /// pops a page. Reports whether anything happened, so a caller wiring a hardware/gesture back can let the
    /// gesture fall through to the OS (exiting the app) when this returns `false` at the root with nothing open.
    ///
    /// Prefer this over [`pop`](Self::pop) for any general back affordance: popping directly would tear the
    /// page out from under an open dialog instead of closing the dialog the user is looking at.
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

    /// Non-subscribing read of the current (top) page, for use inside an event handler.
    pub fn peek_current(&self) -> R {
        self.stack
            .peek_with(|s| s.last().expect("navigator stack is never empty").clone())
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
        self.stack.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    enum Route {
        Home,
        Settings,
        Detail,
    }

    #[test]
    fn starts_at_root() {
        let nav = Navigator::new(Route::Home);
        assert_eq!(nav.current(), Route::Home);
        assert_eq!(nav.depth(), 1);
        assert!(!nav.can_pop());
    }

    #[test]
    fn push_and_pop_track_history() {
        let nav = Navigator::new(Route::Home);
        nav.push(Route::Settings);
        assert_eq!(nav.current(), Route::Settings);
        assert_eq!(nav.depth(), 2);
        assert!(nav.can_pop());

        nav.push(Route::Detail);
        assert_eq!(nav.current(), Route::Detail);

        assert!(nav.pop());
        assert_eq!(nav.current(), Route::Settings);
        assert!(nav.pop());
        assert_eq!(nav.current(), Route::Home);
    }

    #[test]
    fn pop_at_root_is_a_noop() {
        let nav = Navigator::new(Route::Home);
        assert!(!nav.pop());
        assert_eq!(nav.current(), Route::Home);
        assert_eq!(nav.depth(), 1);
    }

    #[test]
    fn replace_swaps_top_without_growing() {
        let nav = Navigator::new(Route::Home);
        nav.push(Route::Settings);
        nav.replace(Route::Detail);
        assert_eq!(nav.current(), Route::Detail);
        assert_eq!(nav.depth(), 2);
    }

    #[test]
    fn pop_to_root_and_reset_clear_the_stack() {
        let nav = Navigator::new(Route::Home);
        nav.push(Route::Settings);
        nav.push(Route::Detail);
        nav.pop_to_root();
        assert_eq!(nav.current(), Route::Home);
        assert_eq!(nav.depth(), 1);

        nav.push(Route::Settings);
        nav.reset(Route::Detail);
        assert_eq!(nav.current(), Route::Detail);
        assert_eq!(nav.depth(), 1);
    }

    #[test]
    fn back_closes_an_open_overlay_before_popping_a_page() {
        let nav = Navigator::new(Route::Home);
        nav.push(Route::Settings);
        let closed = std::rc::Rc::new(std::cell::Cell::new(false));
        let id = {
            let closed = closed.clone();
            ui_core::dismiss::register_dismiss(std::rc::Rc::new(move || closed.set(true)))
        };

        assert!(nav.back(), "the open overlay consumed the back");
        assert!(closed.get(), "the overlay was dismissed");
        assert_eq!(
            nav.current(),
            Route::Settings,
            "the page stack was left alone while a dialog was up"
        );

        assert!(nav.back(), "with nothing open, back pops the page");
        assert_eq!(nav.current(), Route::Home);

        assert!(
            !nav.back(),
            "at the root with nothing open, back is unhandled so the OS gesture can take it"
        );
        ui_core::dismiss::unregister_dismiss(id);
    }

    #[test]
    fn from_signal_adopts_an_existing_stack() {
        let stack = signal(vec![Route::Home, Route::Settings]);
        let nav = Navigator::from_signal(stack.clone(), Route::Home);
        assert_eq!(nav.current(), Route::Settings, "the restored top is kept");
        assert_eq!(nav.depth(), 2);
        nav.push(Route::Detail);
        assert_eq!(
            stack.with(|s| s.len()),
            3,
            "the navigator drives the adopted signal"
        );
    }

    #[test]
    fn from_signal_seeds_an_empty_stack_with_the_root() {
        let nav = Navigator::from_signal(signal(Vec::new()), Route::Home);
        assert_eq!(nav.current(), Route::Home);
        assert_eq!(nav.depth(), 1);
        assert!(!nav.can_pop());
    }

    #[test]
    fn current_is_reactive() {
        let nav = Navigator::new(Route::Home);
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::<Route>::new()));
        let s = seen.clone();
        let n = nav.clone();
        let _e = reactive_core::effect(move || s.borrow_mut().push(n.current()));
        nav.push(Route::Settings);
        nav.pop();
        assert_eq!(
            *seen.borrow(),
            vec![Route::Home, Route::Settings, Route::Home],
            "the effect re-ran on each navigation"
        );
    }
}
