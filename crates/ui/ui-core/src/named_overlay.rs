//! Named overlays: opening a dialog or drawer by name, instead of threading its `signal(bool)` to everything that might open it.
//!
//! Declaratively binding `open:$signal` is still the direct way to drive one overlay from one nearby control. It stops scaling the moment the opener is somewhere else — a menu item three components away, a keyboard shortcut, a deep link, a guard redirect — because the signal has to be created up front and passed down the whole path. Naming the overlay instead (`modal id:"confirm"`) turns opening it into [`open`]`("confirm")` from anywhere in the same surface, which is what makes a dialog a *destination* you navigate to rather than a boolean you wire up.
//!
//! The name is the only new concept: everything else is the existing machinery. The state is a real signal — minted here on first use by either side, so opening a name before its overlay is built works and the overlay picks up the very same signal when it appears — and closing still goes through the dismiss stack, so Escape and `Navigator::back()` already close these with no extra wiring.
//!
//! Names are per surface, like the overlays they open: two canvases showing the same tree each have their own `"confirm"`, and opening it in one leaves the other closed. Code outside a surface opens one of its names with the surface entered, as [`SurfaceCanvas::enter`](crate::SurfaceCanvas::enter) does.

use std::collections::HashMap;

use reactive_core::{RwSignal, current_surface, detached, in_surface_world, signal};

reactive_core::surface_local! {
    /// Per-surface names: a name opened in one surface opens the overlay of that name in that surface alone.
    slot NAMED: HashMap<String, RwSignal<bool>> = HashMap::new();
    access with_named, with_named_ref;
    context NamedOverlaysContext, NamedOverlaysGuard;
}

/// The open-state signal for the overlay named `id` in the active surface, minted on first use.
///
/// Either side may be first: an `open("settings")` from a shortcut handler creates the signal, and the `drawer id:"settings"` built later finds it already true and opens straight away.
pub fn state(id: &str) -> RwSignal<bool> {
    if let Some(existing) = with_named_ref(|named| named.get(id).copied()) {
        return existing;
    }
    // Created outside the map's borrow: minting a signal touches the reactive runtime, which can reach back into anything holding a borrow across it. Owned by the surface rather than by whichever build first asked, so a failed or rebuilt subtree never frees the signal every later `open` writes; the ambient world has no surface to free it with, so its names live as long as the thread.
    let created = if current_surface().is_none() {
        detached(|| signal(false))
    } else {
        in_surface_world(|| signal(false))
    };
    with_named(|named| *named.entry(id.to_string()).or_insert(created))
}

/// Opens the overlay named `id` in the active surface.
///
/// One function covers dialogs and drawers alike: what distinguishes them — the card, the side it is pinned to, its width — is declared on the widget, not chosen at the moment it opens.
pub fn open(id: &str) {
    state(id).set(true);
}

/// Closes the overlay named `id` in the active surface. Note this is *not* the same as a dismissal: it closes exactly this overlay, where `dismiss_top` closes whichever is frontmost.
pub fn close(id: &str) {
    state(id).set(false);
}

#[cfg(test)]
#[path = "named_overlay_test.rs"]
mod tests;
