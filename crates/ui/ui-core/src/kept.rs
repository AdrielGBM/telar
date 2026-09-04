//! State that outlives the tree that reads it.
//!
//! A surface can be *remounted*: its widget tree dropped and built again on the same window, same renderer, same place on screen — how an app follows something that changed underneath it (a config file, a theme, a reloaded dylib) without being replaced by a new one. Everything that lived in the tree goes with it, which for anything the *user* was in the middle of is a bug wearing the shape of a repaint: a search box that empties, a list that jumps back to the top, a transition that plays its entrance again.
//!
//! [`kept`] is where that state goes instead. It lives in the surface's own service scope — the one world that outlives a build and dies with the surface — so two surfaces never share a key, and a closed surface forgets everything it kept.

use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// Everything one surface has asked to keep. Cloning shares it — the `Rc` is what the surface's scope holds.
#[derive(Clone, Default)]
struct Kept(Rc<RefCell<HashMap<&'static str, Rc<dyn Any>>>>);

/// The value this surface keeps under `key`, built by `init` the first time it is asked for and handed back unchanged on every build after that.
///
/// `T` is normally a signal, and keeping the *signal* rather than its value is the point: the rebuilt tree subscribes to the thing the old tree was writing, so a rebuild mid-gesture — a search being typed, a slider being dragged, an animation halfway out — picks up exactly where it was.
///
/// Keys are namespaced by whoever owns them (`"launcher.query"`, `"settings.page"`): one surface can host many components, and two of them reaching for `"query"` would be reaching for the same value. Two live instances of the same component on one surface need two keys for the same reason.
pub fn kept<T: Clone + 'static>(key: &'static str, init: impl FnOnce() -> T) -> T {
    let store = match services_core::try_inject::<Kept>() {
        Some(store) => store,
        None => {
            let store = Kept::default();
            let _ = services_core::provide(store.clone());
            store
        }
    };
    // Read out and release the borrow before `init` runs: what it builds may itself keep something.
    let existing = store
        .0
        .borrow()
        .get(key)
        .and_then(|value| Rc::clone(value).downcast::<T>().ok());
    if let Some(value) = existing {
        return (*value).clone();
    }
    let value = init();
    store
        .0
        .borrow_mut()
        .insert(key, Rc::new(value.clone()) as Rc<dyn Any>);
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use reactive_core::signal;
    use services_core::Scope;

    /// What a remount is, from the state's point of view: the tree is gone, the surface is not, and the value it was reading is still the same one.
    #[test]
    fn a_second_build_reads_back_what_the_first_one_kept() {
        Scope::with(|| {
            let first = kept("test.query", || signal(String::new()));
            first.set("telar".to_string());

            let second = kept("test.query", || signal(String::new()));
            assert_eq!(
                second.peek(),
                "telar",
                "the rebuilt tree subscribes to the signal the old one was writing, not to a fresh one"
            );
            assert_eq!(
                kept("test.other", || signal(String::new())).peek(),
                "",
                "and a different key is a different value"
            );
        });
    }

    /// Two surfaces are two stores, or the second window to open would inherit the first one's search box.
    #[test]
    fn one_surfaces_state_is_not_another_surfaces() {
        let first = Scope::with(|| kept("test.page", || signal(3usize)).peek());
        let second = Scope::with(|| kept("test.page", || signal(0usize)).peek());
        assert_eq!((first, second), (3, 0));
    }
}
