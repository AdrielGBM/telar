//! Named overlays: opening a dialog or drawer by name, instead of threading its `signal(bool)` to everything
//! that might open it.
//!
//! Declaratively binding `open:$signal` is still the direct way to drive one overlay from one nearby control.
//! It stops scaling the moment the opener is somewhere else — a menu item three components away, a keyboard
//! shortcut, a deep link, a guard redirect — because the signal has to be created up front and passed down
//! the whole path. Naming the overlay instead (`modal id:"confirm"`) turns opening it into
//! [`open`]`("confirm")` from anywhere, which is what makes a dialog a *destination* you navigate to rather
//! than a boolean you wire up.
//!
//! The name is the only new concept: everything else is the existing machinery. The state is a real signal —
//! minted here on first use by either side, so opening a name before its overlay is built works and the
//! overlay picks up the very same signal when it appears — and closing still goes through the dismiss stack,
//! so Escape and `Navigator::back()` already close these with no extra wiring.

use std::cell::RefCell;
use std::collections::HashMap;
use std::mem::ManuallyDrop;

use reactive_core::{RwSignal, signal};

// ManuallyDrop keeps this TLS slot trivially-destructible: registering a TLS destructor from a hot-reloaded dylib would make dlclose unsafe (same constraint as `dismiss` and `telar::hot_state`).
thread_local! {
    static NAMED: ManuallyDrop<RefCell<HashMap<String, RwSignal<bool>>>> =
        ManuallyDrop::new(RefCell::new(HashMap::new()));
}

/// The open-state signal for the overlay named `id`, minted on first use.
///
/// Either side may be first: an `open("settings")` from a shortcut handler creates the signal, and the
/// `drawer id:"settings"` built later finds it already true and opens straight away.
pub fn state(id: &str) -> RwSignal<bool> {
    if let Some(existing) = NAMED.with(|named| named.borrow().get(id).cloned()) {
        return existing;
    }
    // Created outside the map's borrow: minting a signal touches the reactive runtime, which can reach back into anything holding a borrow across it.
    let created = signal(false);
    NAMED.with(|named| {
        named
            .borrow_mut()
            .entry(id.to_string())
            .or_insert(created)
            .clone()
    })
}

/// Opens the overlay named `id`.
///
/// One function covers dialogs and drawers alike: what distinguishes them — the card, the side it is pinned
/// to, its width — is declared on the widget, not chosen at the moment it opens.
pub fn open(id: &str) {
    state(id).set(true);
}

/// Closes the overlay named `id`. Note this is *not* the same as a dismissal: it closes exactly this overlay,
/// where `dismiss_top` closes whichever is frontmost.
pub fn close(id: &str) {
    state(id).set(false);
}

pub fn toggle(id: &str) {
    let s = state(id);
    let open = s.peek();
    s.set(!open);
}

/// Reactive read of whether the overlay named `id` is open — for a trigger that styles itself as active while
/// its panel is up.
pub fn is_open(id: &str) -> bool {
    state(id).get()
}

/// Non-subscribing read, for use inside an event handler.
pub fn peek_open(id: &str) -> bool {
    state(id).peek()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reset() {
        NAMED.with(|named| named.borrow_mut().clear());
    }

    #[test]
    fn a_name_resolves_to_one_shared_signal() {
        reset();
        let first = state("confirm");
        let second = state("confirm");
        first.set(true);
        assert!(second.get(), "both handles are the same signal");
        assert!(!state("other").get(), "a different name is independent");
    }

    /// The ordering that makes named overlays worth having: something opens the name before the widget for it
    /// exists (a restored deep link, a shortcut fired during startup), and the widget still comes up open.
    #[test]
    fn opening_a_name_before_its_overlay_is_built_still_opens_it() {
        reset();
        open("settings");
        assert!(state("settings").get());
    }

    #[test]
    fn open_close_and_toggle_drive_the_same_state() {
        reset();
        assert!(!peek_open("panel"));
        open("panel");
        assert!(peek_open("panel"));
        close("panel");
        assert!(!peek_open("panel"));
        toggle("panel");
        assert!(peek_open("panel"));
        toggle("panel");
        assert!(!peek_open("panel"));
    }

    #[test]
    fn is_open_is_reactive() {
        reset();
        let seen = std::rc::Rc::new(RefCell::new(Vec::new()));
        let s = seen.clone();
        let _e = reactive_core::effect(move || s.borrow_mut().push(is_open("banner")));
        open("banner");
        close("banner");
        assert_eq!(*seen.borrow(), vec![false, true, false]);
    }
}
