//! The workshop as the app's address: a [`Navigator`] of [`WorkshopRoute`]s kept in step with where the workshop is and what is set there, and following the platform's location.
//!
//! So a link opens where it points on every target: the location the app was started at (`--location`), a `goto:` line `cargo telar preview <ID>` sends the running workshop, a browser's address. And what the address carries comes back with it: a page reloaded in the browser, or a desktop workshop swapped by a hot reload, opens on the preview, view, args and matrix it showed. Every move replaces the current entry rather than adding one, so moving around the sidebar does not grow a history.

use std::cell::RefCell;
use std::rc::Rc;

use telar::effect;
use telar_navigate::Navigator;

use crate::route::WorkshopRoute;
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
    let opened: Rc<RefCell<Option<WorkshopRoute>>> = Rc::default();
    effect(move || {
        let route = nav.current();
        // Opening reads the state it sets, so a change made in the workshop runs this again before the address has caught up with it; the address it still shows is the one already opened, and must not undo the change.
        if opened.borrow().as_ref() == Some(&route) {
            return;
        }
        opened.replace(Some(route.clone()));
        opening.open(&route);
        // What the workshop could not show as asked, and what a link set that the address does not keep, leave the address for where it is.
        if let Some(shown) = opening.route()
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
