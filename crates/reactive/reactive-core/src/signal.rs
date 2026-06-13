use std::cell::RefCell;
use std::rc::Rc;

use crate::runtime::{self, EffectId};

pub(crate) trait SubscriberSet: 'static {
    // Swap-remove subscriber at `slot`. If the swap moved another subscriber from the last
    // position to `slot`, call update_slot(moved_effect_id, new_slot_in_signal, moved_observer_slot_in_effect_sources).
    fn swap_remove_at(&self, slot: usize, update_slot: &mut dyn FnMut(EffectId, usize, usize));
    // Set the observer_slot for the subscriber at subscriber_slot.
    fn set_observer_slot(&self, subscriber_slot: usize, observer_slot: usize);
}

pub(crate) struct SignalInner<T> {
    pub(crate) value: T,
    pub(crate) subscribers: Vec<EffectId>,
    // For each subscriber: the index in that effect's `source_slots` vec that records this signal.
    // Used by swap_remove_at to tell the moved effect which source_slot entry to update.
    pub(crate) observer_slots: Vec<usize>,
}

impl<T: 'static> SubscriberSet for RefCell<SignalInner<T>> {
    fn swap_remove_at(&self, slot: usize, update_slot: &mut dyn FnMut(EffectId, usize, usize)) {
        let last = {
            let borrow = self.borrow();
            borrow.subscribers.len().saturating_sub(1)
        };
        if slot > last {
            return;
        }
        if slot == last {
            let mut borrow = self.borrow_mut();
            borrow.subscribers.pop();
            borrow.observer_slots.pop();
        } else {
            let (moved_id, moved_obs_slot) = {
                let mut borrow = self.borrow_mut();
                let moved_id = borrow.subscribers[last];
                let moved_obs_slot = borrow.observer_slots[last];
                borrow.subscribers[slot] = moved_id;
                borrow.observer_slots[slot] = moved_obs_slot;
                borrow.subscribers.pop();
                borrow.observer_slots.pop();
                (moved_id, moved_obs_slot)
            };
            // RefCell borrow is released before calling the callback.
            update_slot(moved_id, slot, moved_obs_slot);
        }
    }

    fn set_observer_slot(&self, subscriber_slot: usize, observer_slot: usize) {
        let mut borrow = self.borrow_mut();
        if subscriber_slot < borrow.observer_slots.len() {
            borrow.observer_slots[subscriber_slot] = observer_slot;
        }
    }
}

pub struct ReadSignal<T: 'static> {
    pub(crate) inner: Rc<RefCell<SignalInner<T>>>,
}

impl<T: 'static> Clone for ReadSignal<T> {
    fn clone(&self) -> Self {
        ReadSignal {
            inner: Rc::clone(&self.inner),
        }
    }
}

impl<T: Clone + 'static> ReadSignal<T> {
    pub fn get(&self) -> T {
        track(&self.inner);
        self.inner.borrow().value.clone()
    }

    pub fn peek(&self) -> T {
        self.inner.borrow().value.clone()
    }
}

impl<T: 'static> ReadSignal<T> {
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        track(&self.inner);
        let borrow = self.inner.borrow();
        let result = f(&borrow.value);
        drop(borrow);
        result
    }
}

pub struct WriteSignal<T: 'static> {
    pub(crate) inner: Rc<RefCell<SignalInner<T>>>,
}

impl<T: 'static> Clone for WriteSignal<T> {
    fn clone(&self) -> Self {
        WriteSignal {
            inner: Rc::clone(&self.inner),
        }
    }
}

impl<T: 'static> WriteSignal<T> {
    pub fn set(&self, value: T) {
        self.inner.borrow_mut().value = value;
        notify(&self.inner);
    }

    pub fn update(&self, f: impl FnOnce(&mut T)) {
        let mut borrow = self.inner.borrow_mut();
        f(&mut borrow.value);
        drop(borrow);
        notify(&self.inner);
    }
}

pub struct RwSignal<T: 'static> {
    inner: Rc<RefCell<SignalInner<T>>>,
}

impl<T: 'static> Clone for RwSignal<T> {
    fn clone(&self) -> Self {
        RwSignal {
            inner: Rc::clone(&self.inner),
        }
    }
}

impl<T: 'static> RwSignal<T> {
    pub fn set(&self, value: T) {
        self.inner.borrow_mut().value = value;
        notify(&self.inner);
    }

    pub fn update(&self, f: impl FnOnce(&mut T)) {
        let mut borrow = self.inner.borrow_mut();
        f(&mut borrow.value);
        drop(borrow);
        notify(&self.inner);
    }

    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        track(&self.inner);
        let borrow = self.inner.borrow();
        let result = f(&borrow.value);
        drop(borrow);
        result
    }

    pub fn read_only(&self) -> ReadSignal<T> {
        ReadSignal {
            inner: Rc::clone(&self.inner),
        }
    }

    pub fn write_only(&self) -> WriteSignal<T> {
        WriteSignal {
            inner: Rc::clone(&self.inner),
        }
    }
}

impl<T: Clone + 'static> RwSignal<T> {
    pub fn get(&self) -> T {
        track(&self.inner);
        self.inner.borrow().value.clone()
    }

    pub fn peek(&self) -> T {
        self.inner.borrow().value.clone()
    }
}

pub fn create_signal<T: 'static>(value: T) -> (ReadSignal<T>, WriteSignal<T>) {
    let inner = Rc::new(RefCell::new(SignalInner {
        value,
        subscribers: Vec::new(),
        observer_slots: Vec::new(),
    }));
    (
        ReadSignal {
            inner: Rc::clone(&inner),
        },
        WriteSignal { inner },
    )
}

pub fn create_rw_signal<T: 'static>(value: T) -> RwSignal<T> {
    RwSignal {
        inner: Rc::new(RefCell::new(SignalInner {
            value,
            subscribers: Vec::new(),
            observer_slots: Vec::new(),
        })),
    }
}

fn track<T: 'static>(inner: &Rc<RefCell<SignalInner<T>>>) {
    if let Some(id) = runtime::current_observer() {
        let subscriber_slot = {
            let mut borrow = inner.borrow_mut();
            if borrow.subscribers.contains(&id) {
                return; // already subscribed in this run
            }
            let slot = borrow.subscribers.len();
            borrow.subscribers.push(id);
            borrow.observer_slots.push(0); // placeholder; set_observer_slot will update it
            slot
        };
        let signal_ref = Rc::clone(inner) as Rc<dyn crate::signal::SubscriberSet>;
        runtime::register_source(signal_ref, subscriber_slot);
    }
}

pub(crate) fn notify<T>(inner: &Rc<RefCell<SignalInner<T>>>) {
    let subs: Vec<EffectId> = inner.borrow().subscribers.iter().copied().collect();
    if subs.is_empty() {
        return;
    }
    // Single RUNTIME.with call to schedule all subscribers at once
    let (should_flush, dead): (bool, Vec<EffectId>) = runtime::with_runtime_mut(|rt| {
        let mut dead: Vec<EffectId> = Vec::new();
        let mut any_scheduled = false;
        for &id in &subs {
            if rt.effects.contains(id) {
                if rt.pending_set.insert(id) {
                    if rt.effects[id].pure {
                        rt.memo_pending.push(id);
                    } else {
                        rt.pending.push(id);
                    }
                }
                any_scheduled = true;
            } else {
                dead.push(id);
            }
        }
        (any_scheduled && rt.batch_depth == 0 && !rt.flushing, dead)
    });
    // Clean up dead subscribers outside of RUNTIME borrow
    if !dead.is_empty() {
        let mut borrow = inner.borrow_mut();
        // Remove dead subscribers, keeping observers_slots in sync
        let mut i = 0;
        while i < borrow.subscribers.len() {
            if dead.contains(&borrow.subscribers[i]) {
                borrow.subscribers.swap_remove(i);
                borrow.observer_slots.swap_remove(i);
            } else {
                i += 1;
            }
        }
    }
    if should_flush {
        runtime::flush_public();
    }
}
