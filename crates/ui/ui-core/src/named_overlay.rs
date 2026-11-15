//! Named overlays: opening a dialog or drawer by name, instead of threading its `signal(bool)` to everything that might open it.
//!
//! Binding `control:$signal` is still the direct way to drive one overlay from one nearby control. It stops scaling the moment the opener is somewhere else — a menu item three components away, a keyboard shortcut, a deep link, a guard redirect — because the signal has to be created up front and passed down the whole path. Naming the overlay instead turns opening it into [`open`]`(CONFIRM)` from anywhere in the same surface, which is what makes a dialog a *destination* you navigate to rather than a boolean you wire up. The name is an [`OverlayName`] constant declared once and named by the overlay (`modal control:CONFIRM`) and by every opener alike, so a misspelt name is a compile error rather than an overlay that never opens.
//!
//! The name is the only new concept: everything else is the existing machinery. The state is a real signal — minted here on first use by either side, so opening a name before its overlay is built works and the overlay picks up the very same signal when it appears — and closing still goes through the dismiss stack, so Escape and `Navigator::back()` already close these with no extra wiring.
//!
//! Names are per surface, like the overlays they open: two canvases showing the same tree each have their own `CONFIRM`, and opening it in one leaves the other closed. Code outside a surface opens one of its names with the surface entered, through [`SurfaceCanvas::with_entered`](crate::SurfaceCanvas::with_entered).

use std::collections::HashMap;

use reactive_core::{RwSignal, current_surface, detached, in_surface_world, signal};

/// The name an overlay is opened by: declared once, as a constant both the overlay and every opener name, so a misspelt name fails to compile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct OverlayName(&'static str);

impl OverlayName {
    /// A name spelt `name`. Two constants with the same spelling name the same overlay.
    pub const fn new(name: &'static str) -> Self {
        Self(name)
    }

    /// How the name is spelt.
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

/// What opens and closes an overlay: the shared state of a name anything on the surface can open, or a signal its caller holds.
#[derive(Clone, Copy)]
pub enum OverlayControl {
    /// Opened and closed through [`open`] and [`close`] with this name, from anywhere on the same surface.
    Named(OverlayName),
    /// Opened and closed by writing this signal.
    Signal(RwSignal<bool>),
}

impl OverlayControl {
    /// The open-state signal this control drives, a named one resolved in the active surface.
    pub fn state(self) -> RwSignal<bool> {
        match self {
            Self::Named(name) => state(name),
            Self::Signal(open) => open,
        }
    }
}

impl From<OverlayName> for OverlayControl {
    fn from(name: OverlayName) -> Self {
        Self::Named(name)
    }
}

impl From<RwSignal<bool>> for OverlayControl {
    fn from(open: RwSignal<bool>) -> Self {
        Self::Signal(open)
    }
}

reactive_core::surface_local! {
    /// Per-surface names: a name opened in one surface opens the overlay of that name in that surface alone.
    slot NAMED: HashMap<OverlayName, RwSignal<bool>> = HashMap::new();
    access with_named, with_named_ref;
    context NamedOverlaysContext;
}

/// The open-state signal for the overlay named `name` in the active surface, minted on first use.
///
/// Either side may be first: an `open(SETTINGS)` from a shortcut handler creates the signal, and the `drawer control:SETTINGS` built later finds it already true and opens straight away.
pub fn state(name: OverlayName) -> RwSignal<bool> {
    if let Some(existing) = with_named_ref(|named| named.get(&name).copied()) {
        return existing;
    }
    // Created outside the map's borrow: minting a signal touches the reactive runtime, which can reach back into anything holding a borrow across it. Owned by the surface rather than by whichever build first asked, so a failed or rebuilt subtree never frees the signal every later `open` writes; the ambient world has no surface to free it with, so its names live as long as the thread.
    let created = if current_surface().is_none() {
        detached(|| signal(false))
    } else {
        in_surface_world(|| signal(false))
    };
    with_named(|named| *named.entry(name).or_insert(created))
}

/// Opens the overlay named `name` in the active surface.
///
/// One function covers dialogs and drawers alike: what distinguishes them — the card, the side it is pinned to, its width — is declared on the widget, not chosen at the moment it opens.
pub fn open(name: OverlayName) {
    state(name).set(true);
}

/// Closes the overlay named `name` in the active surface. Note this is *not* the same as a dismissal: it closes exactly this overlay, where `dismiss_top` closes whichever is frontmost.
pub fn close(name: OverlayName) {
    state(name).set(false);
}

#[cfg(test)]
#[path = "named_overlay.test.rs"]
mod tests;
