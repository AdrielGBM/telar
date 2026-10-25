//! A `RefCell` that remembers where its outermost live borrow was taken, so a collision can name the borrow it ran into.

use std::cell::{Cell, Ref, RefCell, RefMut};
use std::fmt;
use std::panic::Location;

pub(crate) struct SiteCell<T> {
    value: RefCell<T>,
    held_at: Cell<Option<&'static Location<'static>>>,
}

/// Where the borrow a collision ran into was taken.
pub(crate) struct HeldAt(Option<&'static Location<'static>>);

impl fmt::Display for HeldAt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(site) => site.fmt(f),
            None => f.write_str("unknown"),
        }
    }
}

impl<T> SiteCell<T> {
    pub(crate) const fn new(value: T) -> Self {
        SiteCell {
            value: RefCell::new(value),
            held_at: Cell::new(None),
        }
    }

    #[track_caller]
    pub(crate) fn try_read(&self) -> Result<Ref<'_, T>, HeldAt> {
        // A nested shared borrow must not overwrite the site of the one still holding the value, which is the one a later collision has to name.
        let outermost = self.is_free();
        match self.value.try_borrow() {
            Ok(value) => {
                if outermost {
                    self.held_at.set(Some(Location::caller()));
                }
                Ok(value)
            }
            Err(_) => Err(HeldAt(self.held_at.get())),
        }
    }

    #[track_caller]
    pub(crate) fn try_write(&self) -> Result<RefMut<'_, T>, HeldAt> {
        match self.value.try_borrow_mut() {
            Ok(value) => {
                self.held_at.set(Some(Location::caller()));
                Ok(value)
            }
            Err(_) => Err(HeldAt(self.held_at.get())),
        }
    }

    pub(crate) fn is_free(&self) -> bool {
        self.value.try_borrow_mut().is_ok()
    }
}
