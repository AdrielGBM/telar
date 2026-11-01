//! The value handles of a keyed reconcile: one signal per key, so an item that keeps its key follows what it now carries instead of being rebuilt for it.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use reactive_core::{ReadSignal, RwSignal, on_cleanup, signal};

/// Each key's handle, with the token of the item that created it.
type Handles<Item> = HashMap<u64, (u64, RwSignal<Item>)>;

pub(crate) struct KeyedValues<Item: 'static> {
    values: Rc<RefCell<Handles<Item>>>,
    next_token: Rc<Cell<u64>>,
}

impl<Item: 'static> Clone for KeyedValues<Item> {
    fn clone(&self) -> Self {
        Self {
            values: Rc::clone(&self.values),
            next_token: Rc::clone(&self.next_token),
        }
    }
}

impl<Item: Clone + 'static> KeyedValues<Item> {
    pub(crate) fn new() -> Self {
        Self {
            values: Rc::new(RefCell::new(HashMap::new())),
            next_token: Rc::new(Cell::new(0)),
        }
    }

    /// Moves the handle of the item under `key`, if one is built, to `item`: called before the reuse decision, so an item keeping its key sees the value it now carries.
    pub(crate) fn sync(&self, item: &Item, key: u64) {
        let existing = self.values.borrow().get(&key).map(|(_, value)| *value);
        if let Some(existing) = existing {
            existing.set(item.clone());
        }
    }

    /// A handle on `item` for the item being built under `key`, created in that build so it belongs to the item's owner and is freed with it.
    pub(crate) fn hold(&self, item: Item, key: u64) -> ReadSignal<Item> {
        let token = self
            .next_token
            .replace(self.next_token.get().wrapping_add(1));
        let held = signal(item);
        self.values.borrow_mut().insert(key, (token, held));
        let values = Rc::downgrade(&self.values);
        // The token tells this cleanup whether the entry is still its own: a key rebuilt before the old item is disposed holds a newer handle.
        on_cleanup(move || {
            let Some(values) = values.upgrade() else {
                return;
            };
            let mut values = values.borrow_mut();
            if values.get(&key).is_some_and(|(owner, _)| *owner == token) {
                values.remove(&key);
            }
        });
        held.read_only()
    }
}
