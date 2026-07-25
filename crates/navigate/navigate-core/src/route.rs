use crate::navigator::Navigator;
use crate::tabs::TabStacks;

/// The optional path layer over an app's typed route enum.
///
/// The enum stays the primary model — that is what makes navigation type-safe, exhaustively matched and
/// autocompleted. This trait is only an adapter for the places a route has to survive as text: a deep link, a
/// restored session, a window title, a log line. Implement it when you need one of those; a route type
/// without it navigates exactly the same.
///
/// ```ignore
/// impl Routable for Route {
///     fn to_path(&self) -> String {
///         match self {
///             Route::Home => "/".into(),
///             Route::Settings => "/settings".into(),
///             Route::Post(id) => format!("/post/{id}"),
///         }
///     }
///     fn from_path(path: &str) -> Option<Self> {
///         match path {
///             "/" => Some(Route::Home),
///             "/settings" => Some(Route::Settings),
///             _ => path.strip_prefix("/post/")?.parse().ok().map(Route::Post),
///         }
///     }
/// }
/// ```
pub trait Routable: Sized {
    /// The path this route serialises to.
    fn to_path(&self) -> String;

    /// Parses a path back into a route, or `None` when it names nothing this app knows. Returning `None` is
    /// how a stale bookmark or a hand-typed link is rejected rather than navigated to.
    fn from_path(path: &str) -> Option<Self>;
}

/// A literal path table for route types that carry no data — the common case, where every destination is a
/// bare variant and the mapping is one line each.
///
/// Saves hand-writing both halves of [`Routable`] and, more usefully, keeps them from drifting apart: one
/// table serves both directions, so a renamed path cannot parse to one route and serialise to another.
///
/// ```ignore
/// let table = RouteTable::new([("/", Route::Home), ("/settings", Route::Settings)]);
/// assert_eq!(table.route_of("/settings"), Some(Route::Settings));
/// assert_eq!(table.path_of(&Route::Home), Some("/"));
/// ```
pub struct RouteTable<R: Clone + Eq> {
    entries: Vec<(&'static str, R)>,
}

impl<R: Clone + Eq> RouteTable<R> {
    pub fn new(entries: impl IntoIterator<Item = (&'static str, R)>) -> Self {
        Self {
            entries: entries.into_iter().collect(),
        }
    }

    /// The path a route serialises to. The first entry naming that route wins, so listing an alias after the
    /// canonical path keeps the canonical one as what gets written out.
    pub fn path_of(&self, route: &R) -> Option<&'static str> {
        self.entries
            .iter()
            .find(|(_, r)| r == route)
            .map(|(path, _)| *path)
    }

    /// The route a path names, or `None` when the table has no such entry.
    pub fn route_of(&self, path: &str) -> Option<R> {
        self.entries
            .iter()
            .find(|(p, _)| *p == path)
            .map(|(_, r)| r.clone())
    }

    /// Every `(path, route)` pair, in declaration order.
    pub fn entries(&self) -> impl Iterator<Item = (&'static str, &R)> {
        self.entries.iter().map(|(path, route)| (*path, route))
    }
}

impl<R: Clone + Routable + 'static> Navigator<R> {
    /// The path of the screen on top — for a window title, a log line, or the link to hand back to the user.
    pub fn current_path(&self) -> String {
        self.current().to_path()
    }

    /// The whole stack as paths, root-first. This, not [`current_path`](Self::current_path), is what a
    /// session restore should keep: a deep link that only carries the destination drops the user somewhere
    /// with no way back.
    pub fn path_stack(&self) -> Vec<String> {
        self.with_stack(|stack| stack.iter().map(Routable::to_path).collect())
    }

    /// Pushes the screen `path` names, reporting whether it parsed. An unknown path navigates nowhere.
    pub fn push_path(&self, path: &str) -> bool {
        match R::from_path(path) {
            Some(route) => {
                self.push(route);
                true
            }
            None => false,
        }
    }

    /// Replaces the current screen with the one `path` names, reporting whether it parsed.
    pub fn replace_path(&self, path: &str) -> bool {
        match R::from_path(path) {
            Some(route) => {
                self.replace(route);
                true
            }
            None => false,
        }
    }

    /// Restores a whole stack from paths, root-first — the other half of [`path_stack`](Self::path_stack).
    ///
    /// All-or-nothing on purpose: a partially parsed stack is worse than a rejected one, because silently
    /// dropping an unrecognised entry can leave the user on a detail screen whose parent never existed. Also
    /// rejects an empty list, which would break the navigator's never-empty invariant.
    pub fn restore_paths(&self, paths: &[&str]) -> bool {
        if paths.is_empty() {
            return false;
        }
        let mut routes = Vec::with_capacity(paths.len());
        for path in paths {
            match R::from_path(path) {
                Some(route) => routes.push(route),
                None => return false,
            }
        }
        self.signal().set(routes);
        true
    }
}

impl<T: Clone + Eq + 'static, R: Clone + 'static> TabStacks<T, R> {
    /// Opens a deep link into nested stacks: selects `tab` and makes `stack` (root-first) its whole history,
    /// so the user lands on the destination *with* the screens above it already behind them.
    ///
    /// Reports whether it did anything; a tab outside the declared set, or an empty stack, is rejected rather
    /// than half-applied. Note this bypasses [`select`](Self::select)'s pop-to-root, since a deep link into
    /// the tab you are already on should still land where the link points.
    pub fn deep_link(&self, tab: T, stack: Vec<R>) -> bool {
        if stack.is_empty() {
            return false;
        }
        let Some(nav) = self.navigator_for(&tab) else {
            return false;
        };
        nav.signal().set(stack);
        let leaving = self.peek_active();
        if leaving != tab {
            // Recorded like any switch, so Back returns to whatever the user was doing when the link landed; on a cold start the history is empty, so there is nothing to record and nothing to return to.
            self.remember(leaving);
        }
        self.active_signal().set(tab);
        true
    }
}

#[cfg(test)]
mod tests {
    use reactive_core::signal;

    use super::*;

    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    enum Route {
        Home,
        Settings,
        Post(u32),
    }

    impl Routable for Route {
        fn to_path(&self) -> String {
            match self {
                Route::Home => "/".into(),
                Route::Settings => "/settings".into(),
                Route::Post(id) => format!("/post/{id}"),
            }
        }

        fn from_path(path: &str) -> Option<Self> {
            match path {
                "/" => Some(Route::Home),
                "/settings" => Some(Route::Settings),
                _ => path.strip_prefix("/post/")?.parse().ok().map(Route::Post),
            }
        }
    }

    #[test]
    fn a_stack_round_trips_through_paths() {
        let nav = Navigator::new(Route::Home);
        nav.push(Route::Settings);
        nav.push(Route::Post(42));
        assert_eq!(nav.current_path(), "/post/42");
        assert_eq!(nav.path_stack(), vec!["/", "/settings", "/post/42"]);

        let restored = Navigator::new(Route::Home);
        assert!(restored.restore_paths(&["/", "/settings", "/post/42"]));
        assert_eq!(restored.current(), Route::Post(42));
        assert_eq!(
            restored.depth(),
            3,
            "the screens above the destination came back too, so Back still works"
        );
    }

    #[test]
    fn an_unknown_path_navigates_nowhere() {
        let nav = Navigator::new(Route::Home);
        assert!(!nav.push_path("/nope"));
        assert_eq!(nav.depth(), 1);
        assert!(nav.push_path("/settings"));
        assert_eq!(nav.current(), Route::Settings);
        assert!(!nav.replace_path("/nope"));
        assert_eq!(nav.current(), Route::Settings);
    }

    /// A stale bookmark whose middle entry no longer parses must be rejected whole: applying the prefix would
    /// strand the user on a detail screen whose parent never existed.
    #[test]
    fn a_partly_unparseable_stack_is_rejected_whole() {
        let nav = Navigator::new(Route::Home);
        nav.push(Route::Settings);
        assert!(!nav.restore_paths(&["/", "/gone", "/post/1"]));
        assert_eq!(
            nav.current(),
            Route::Settings,
            "the live stack is untouched"
        );
        assert_eq!(nav.depth(), 2);

        assert!(!nav.restore_paths(&[]), "an empty stack is not a stack");
        assert_eq!(nav.depth(), 2);
    }

    #[test]
    fn a_route_table_maps_both_ways_from_one_declaration() {
        let table = RouteTable::new([("/", Route::Home), ("/settings", Route::Settings)]);
        assert_eq!(table.route_of("/settings"), Some(Route::Settings));
        assert_eq!(table.path_of(&Route::Home), Some("/"));
        assert_eq!(table.route_of("/missing"), None);
        assert_eq!(table.path_of(&Route::Post(1)), None);
        assert_eq!(table.entries().count(), 2);
    }

    #[test]
    fn a_route_table_writes_out_the_canonical_path_for_an_aliased_route() {
        let table = RouteTable::new([("/", Route::Home), ("/home", Route::Home)]);
        assert_eq!(
            table.route_of("/home"),
            Some(Route::Home),
            "the alias parses"
        );
        assert_eq!(
            table.path_of(&Route::Home),
            Some("/"),
            "but the first-listed path is the one written back out"
        );
    }

    #[test]
    fn a_deep_link_lands_in_a_tab_with_its_history_behind_it() {
        let stacks = TabStacks::new(signal(0u8), &[0, 1], |_| Navigator::new(Route::Home));
        assert!(stacks.deep_link(1, vec![Route::Home, Route::Post(7)]));
        assert_eq!(stacks.peek_active(), 1);
        assert_eq!(stacks.navigator().current(), Route::Post(7));
        assert!(stacks.back(), "the root came with it, so Back works");
        assert_eq!(stacks.navigator().current(), Route::Home);
    }

    #[test]
    fn a_deep_link_to_an_undeclared_tab_is_rejected() {
        let stacks = TabStacks::new(signal(0u8), &[0, 1], |_| Navigator::new(Route::Home));
        assert!(!stacks.deep_link(9, vec![Route::Settings]));
        assert!(!stacks.deep_link(1, vec![]), "an empty stack is rejected");
        assert_eq!(stacks.peek_active(), 0, "neither one moved the user");
    }
}
