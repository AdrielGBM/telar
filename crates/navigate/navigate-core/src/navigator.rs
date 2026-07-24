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
