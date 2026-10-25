//! [`SurfaceReport`]: what a guest's surface reports back to the host that draws it, the one part of every entry table both boundaries declare alike.

use super::contract::TypeLayout;
use super::lent::{Handle, LentBytes};
use super::outcome::{CallOutcome, guest_entry};
use crate::{AccessNode, ConsumedKeys, Event, TextInput};

/// The guest's side of a surface its host draws as a foreign one, each answer in the surface's own coordinates and ids. The host keeps one focus stop for the surface, so exactly one side has a focus owner: focus arrives from the host, moves inside on Tab, leaves on Tab past either end, and is cleared when the stop loses it.
pub trait ReportsSurface: 'static {
    /// A control's id is its own focus id below `1 << 32`, or one a surface nested in it qualified with its handle.
    fn access_snapshot(&self) -> Vec<AccessNode>;

    fn perform_access_action(&mut self, id: u64, activate: bool);

    fn text_input(&self) -> Option<TextInput>;

    /// Hands focus to the first focusable, or the last when `backwards`, unless one holds it; answers whether one does.
    fn arrive(&mut self, backwards: bool) -> bool;

    /// Answers whether focus stepped past either end, which leaves nothing focused inside for the host to carry on from its stop.
    fn tab(&mut self, key: &Event) -> bool;

    fn blur(&mut self);

    fn holds_focus(&self) -> bool;

    /// What the host's shortcut tables and dismiss stack ask before they act on a key its stop holds.
    fn text_entry_focused(&self) -> bool;

    /// Whether the control holding focus inside keeps `keys` for itself, so the host's dismiss stack leaves them to it.
    fn keeps(&self, keys: ConsumedKeys) -> bool;
}

/// Declares [`SurfaceReport`], its `serving` constructor and the layout the stamp hashes from one list, so an entry's signature is hashed as it is declared.
macro_rules! surface_report {
    ($($field:ident: fn($($arg:ty),* $(,)?);)*) => {
        /// The entries through which the host reads a [`ReportsSurface`] a guest keeps behind a `Handle<T>`, part of every [`Entries`](super::Entries) table; each answers through the pointers it is handed, a rich value lent as bytes.
        #[repr(C)]
        pub struct SurfaceReport<T> {
            $(pub $field: unsafe extern "C" fn(Handle<T>, $($arg),*) -> CallOutcome,)*
        }

        impl<T: ReportsSurface> SurfaceReport<T> {
            pub const fn serving() -> Self {
                Self { $($field: $field::<T>,)* }
            }
        }

        /// The handle's type changes nothing of it.
        pub(super) const SURFACE_REPORT_LAYOUT: TypeLayout = TypeLayout {
            name: "SurfaceReport",
            size: std::mem::size_of::<SurfaceReport<()>>(),
            align: std::mem::align_of::<SurfaceReport<()>>(),
            fields: &[$((
                concat!(stringify!($field), ": fn(Handle, ", $(stringify!($arg), ", ",)* ")"),
                std::mem::offset_of!(SurfaceReport<()>, $field),
            ),)*],
        };
    };
}

surface_report! {
    access_snapshot: fn(*mut LentBytes);
    access_action: fn(u64, bool);
    text_input: fn(*mut LentBytes);
    arrive: fn(bool, *mut bool);
    tab: fn(LentBytes, *mut bool);
    blur: fn();
    holds_focus: fn(*mut bool);
    text_entry_focused: fn(*mut bool);
    keeps: fn(u16, *mut bool);
}

impl<T> Clone for SurfaceReport<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for SurfaceReport<T> {}

unsafe extern "C" fn access_snapshot<T: ReportsSurface>(
    surface: Handle<T>,
    nodes: *mut LentBytes,
) -> CallOutcome {
    guest_entry(|| {
        let read = unsafe { surface.get() }.access_snapshot();
        unsafe { nodes.write(LentBytes::answer(&read)) };
    })
}

unsafe extern "C" fn access_action<T: ReportsSurface>(
    surface: Handle<T>,
    id: u64,
    activate: bool,
) -> CallOutcome {
    guest_entry(|| unsafe { surface.get() }.perform_access_action(id, activate))
}

unsafe extern "C" fn text_input<T: ReportsSurface>(
    surface: Handle<T>,
    input: *mut LentBytes,
) -> CallOutcome {
    guest_entry(|| {
        let caret = unsafe { surface.get() }.text_input();
        unsafe { input.write(LentBytes::answer(&caret)) };
    })
}

unsafe extern "C" fn arrive<T: ReportsSurface>(
    surface: Handle<T>,
    backwards: bool,
    took: *mut bool,
) -> CallOutcome {
    guest_entry(|| {
        let taken = unsafe { surface.get() }.arrive(backwards);
        unsafe { took.write(taken) };
    })
}

unsafe extern "C" fn tab<T: ReportsSurface>(
    surface: Handle<T>,
    key: LentBytes,
    departed: *mut bool,
) -> CallOutcome {
    guest_entry(|| {
        let key: Event = unsafe { key.read() };
        let left = unsafe { surface.get() }.tab(&key);
        unsafe { departed.write(left) };
    })
}

unsafe extern "C" fn blur<T: ReportsSurface>(surface: Handle<T>) -> CallOutcome {
    guest_entry(|| unsafe { surface.get() }.blur())
}

unsafe extern "C" fn holds_focus<T: ReportsSurface>(
    surface: Handle<T>,
    holds: *mut bool,
) -> CallOutcome {
    guest_entry(|| {
        let held = unsafe { surface.get() }.holds_focus();
        unsafe { holds.write(held) };
    })
}

unsafe extern "C" fn text_entry_focused<T: ReportsSurface>(
    surface: Handle<T>,
    focused: *mut bool,
) -> CallOutcome {
    guest_entry(|| {
        let typing = unsafe { surface.get() }.text_entry_focused();
        unsafe { focused.write(typing) };
    })
}

unsafe extern "C" fn keeps<T: ReportsSurface>(
    surface: Handle<T>,
    keys: u16,
    kept: *mut bool,
) -> CallOutcome {
    guest_entry(|| {
        let keeps = unsafe { surface.get() }.keeps(ConsumedKeys::from_bits(keys));
        unsafe { kept.write(keeps) };
    })
}

#[cfg(test)]
#[path = "surface_test.rs"]
mod tests;
