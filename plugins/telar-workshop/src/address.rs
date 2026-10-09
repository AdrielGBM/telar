//! The workshop as the app's address: a [`Navigator`] of [`WorkshopRoute`]s kept in step with the selection, the view and the chrome, and following the platform's location.
//!
//! So a link opens where it points on every target: the location the app was started at (`--location`), a `goto:` line `cargo telar preview <ID>` sends the running workshop, a browser's address. Selecting another preview replaces the current entry rather than adding one, so moving around the sidebar does not grow a history.

use telar::effect;
use telar_navigate::Navigator;

use crate::state::WorkshopState;

/// Keeps `state` and the app's address in step for as long as the owner this is called under lives. A workshop with no previews has no address.
pub(crate) fn follow(state: &WorkshopState) {
    let Some(here) = state.current_route() else {
        return;
    };
    let nav = Navigator::new(here);
    let watched = state.clone();
    effect(move || {
        let Some(route) = watched.route() else {
            return;
        };
        if nav.peek_stack(|stack| stack.last() != Some(&route)) {
            nav.replace(route);
        }
    });
    let opening = state.clone();
    effect(move || {
        let route = nav.current();
        opening.open(&route);
        // What the workshop could not show as asked, and the link it has now set, leave the address for where it is.
        if let Some(shown) = opening.current_route()
            && shown != route
        {
            nav.replace(shown);
        }
    });
    nav.follow_location();
}

#[cfg(test)]
#[path = "address_test.rs"]
mod tests;
