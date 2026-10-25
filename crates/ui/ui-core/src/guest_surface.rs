//! [`GuestSurface`]: the surface an instance of a guest library draws, as its host reads it through the [`SurfaceReport`](platform_core::guest::SurfaceReport) both boundaries share.

use std::cell::Cell;
use std::rc::Rc;

use platform_core::guest::{CallOutcome, Entries, Handle, LentBytes};
use platform_core::{AccessAudit, AccessNode, ConsumedKeys, Event, TextInput};
use renderer_core::Color;
use services_core::guest::GuestLibrary;
use wire_core::encode;

use crate::foreign_surface::ForeignSurface;

/// An instance's guest-side check of what a reader is told about it, as an inspector asks for one: the one answer of [`ForeignSurface`] not every boundary has an entry for.
pub type GuestAudit<E> =
    fn(&GuestLibrary<E>, Handle<<E as Entries>::Surface>, Option<Color>) -> Option<AccessAudit>;

/// The [`ForeignSurface`] of one instance a guest library keeps behind `surface`, read through the library's [`SurfaceReport`](platform_core::guest::SurfaceReport): what hot reload and embed both mark their guest's frame with.
///
/// Once the guest has panicked, or once its owner [forgets](Self::forget) the instance, it reports nothing and takes nothing: no node, no caret, no focus.
pub struct GuestSurface<E: Entries> {
    library: Rc<GuestLibrary<E>>,
    surface: Cell<Option<Handle<E::Surface>>>,
    audit: Option<GuestAudit<E>>,
}

impl<E: Entries> GuestSurface<E> {
    pub fn new(library: Rc<GuestLibrary<E>>, surface: Handle<E::Surface>) -> Self {
        Self {
            library,
            surface: Cell::new(Some(surface)),
            audit: None,
        }
    }

    /// The surface with `audit` answering [`ForeignSurface::access_audit`].
    pub fn audited_by(self, audit: GuestAudit<E>) -> Self {
        Self {
            audit: Some(audit),
            ..self
        }
    }

    /// Stops reading the instance, which its owner is about to release: a handle is handed to its library's release entry exactly once and never used after.
    pub fn forget(&self) {
        self.surface.set(None);
    }

    fn ask(&self, entry: impl FnOnce(&E, Handle<E::Surface>, *mut bool) -> CallOutcome) -> bool {
        let Some(surface) = self.surface.get() else {
            return false;
        };
        self.library
            .ask(|entries, out| entry(entries, surface, out))
    }

    fn read<T: wire_core::Wire + Default>(
        &self,
        entry: impl FnOnce(&E, Handle<E::Surface>, *mut LentBytes) -> CallOutcome,
    ) -> T {
        let Some(surface) = self.surface.get() else {
            return T::default();
        };
        self.library
            .read(|entries, out| entry(entries, surface, out))
            .unwrap_or_default()
    }

    fn call(&self, entry: impl FnOnce(&E, Handle<E::Surface>) -> CallOutcome) {
        if let Some(surface) = self.surface.get() {
            self.library.call(|entries| entry(entries, surface));
        }
    }
}

impl<E: Entries> ForeignSurface for GuestSurface<E> {
    fn access_snapshot(&self) -> Vec<AccessNode> {
        self.read(|entries, surface, nodes| unsafe {
            (entries.report().access_snapshot)(surface, nodes)
        })
    }

    fn perform_access_action(&self, id: u64, activate: bool) {
        self.call(|entries, surface| unsafe {
            (entries.report().access_action)(surface, id, activate)
        });
    }

    fn text_input(&self) -> Option<TextInput> {
        self.read(|entries, surface, input| unsafe {
            (entries.report().text_input)(surface, input)
        })
    }

    fn arrive(&self, backwards: bool) -> bool {
        self.ask(|entries, surface, took| unsafe {
            (entries.report().arrive)(surface, backwards, took)
        })
    }

    fn tab(&self, key: &Event) -> bool {
        let key = encode(key);
        self.ask(|entries, surface, departed| unsafe {
            (entries.report().tab)(surface, LentBytes::lend(&key), departed)
        })
    }

    fn blur(&self) {
        self.call(|entries, surface| unsafe { (entries.report().blur)(surface) });
    }

    fn holds_focus(&self) -> bool {
        self.ask(|entries, surface, holds| unsafe {
            (entries.report().holds_focus)(surface, holds)
        })
    }

    fn text_entry_focused(&self) -> bool {
        self.ask(|entries, surface, typing| unsafe {
            (entries.report().text_entry_focused)(surface, typing)
        })
    }

    fn keeps(&self, keys: ConsumedKeys) -> bool {
        self.ask(|entries, surface, kept| unsafe {
            (entries.report().keeps)(surface, keys.bits(), kept)
        })
    }

    fn access_audit(&self, background: Option<Color>) -> Option<AccessAudit> {
        (self.audit?)(&self.library, self.surface.get()?, background)
    }
}

#[cfg(test)]
#[path = "guest_surface_test.rs"]
mod tests;
