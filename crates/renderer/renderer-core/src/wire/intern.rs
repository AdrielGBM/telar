//! Shared payloads cross once and are then named by slot, on both sides, for as long as consecutive frames keep drawing them.
//!
//! A payload is found by its address first and by its bytes second, so a guest that rebuilds an equal style every frame still sends it once. Both sides keep a slot exactly while the last frame used it, so the decoder mirrors the encoder's table without the frame saying what was dropped.

use std::sync::Arc;

use rustc_hash::FxHashMap;
use xxhash_rust::xxh3::xxh3_128;

use super::WireError;
use super::bytes::{Reader, Writer};

struct Seen<T: ?Sized> {
    slot: u32,
    frame: u64,
    // Holding the value keeps its address from being reused by another one while the address names the slot.
    _value: Arc<T>,
}

struct Slot {
    content: u128,
    used: u64,
}

pub(crate) struct Interner<T: ?Sized> {
    by_address: FxHashMap<usize, Seen<T>>,
    // A 128-bit hash of the spelled bytes stands in for comparing them: a collision is as unlikely as a corrupted read.
    by_content: FxHashMap<u128, u32>,
    slots: Vec<Option<Slot>>,
    free: Vec<u32>,
    scratch: Writer,
}

impl<T: ?Sized> Default for Interner<T> {
    fn default() -> Self {
        Self {
            by_address: FxHashMap::default(),
            by_content: FxHashMap::default(),
            slots: Vec::new(),
            free: Vec::new(),
            scratch: Writer::default(),
        }
    }
}

impl<T: ?Sized> Interner<T> {
    pub(crate) fn put(
        &mut self,
        w: &mut Writer,
        value: &Arc<T>,
        frame: u64,
        spell: impl FnOnce(&mut Writer, &T),
    ) {
        let address = Arc::as_ptr(value).cast::<()>() as usize;
        if let Some(seen) = self.by_address.get_mut(&address)
            && let Some(slot) = &mut self.slots[seen.slot as usize]
        {
            seen.frame = frame;
            slot.used = frame;
            w.varint(u64::from(seen.slot) << 1);
            return;
        }
        self.scratch.clear();
        spell(&mut self.scratch, value);
        let content = xxh3_128(self.scratch.as_slice());
        let slot = match self.by_content.get(&content) {
            Some(&slot) => {
                if let Some(held) = &mut self.slots[slot as usize] {
                    held.used = frame;
                }
                w.varint(u64::from(slot) << 1);
                slot
            }
            None => {
                let slot = self.allocate(content, frame);
                w.varint(u64::from(slot) << 1 | 1);
                w.raw(self.scratch.as_slice());
                slot
            }
        };
        self.by_address.insert(
            address,
            Seen {
                slot,
                frame,
                _value: Arc::clone(value),
            },
        );
    }

    fn allocate(&mut self, content: u128, used: u64) -> u32 {
        let slot = self.free.pop().unwrap_or_else(|| {
            self.slots.push(None);
            (self.slots.len() - 1) as u32
        });
        self.slots[slot as usize] = Some(Slot { content, used });
        self.by_content.insert(content, slot);
        slot
    }

    pub(crate) fn sweep(&mut self, frame: u64) {
        for (index, entry) in self.slots.iter_mut().enumerate() {
            if let Some(slot) = entry.take_if(|slot| slot.used != frame) {
                self.by_content.remove(&slot.content);
                self.free.push(index as u32);
            }
        }
        self.by_address.retain(|_, seen| seen.frame == frame);
    }

    pub(crate) fn clear(&mut self) {
        self.by_address.clear();
        self.by_content.clear();
        self.slots.clear();
        self.free.clear();
    }
}

pub(crate) struct Slots<T: ?Sized> {
    live: Vec<Option<(Arc<T>, u64)>>,
}

impl<T: ?Sized> Default for Slots<T> {
    fn default() -> Self {
        Self { live: Vec::new() }
    }
}

impl<T: ?Sized> Slots<T> {
    pub(crate) fn take(
        &mut self,
        r: &mut Reader<'_>,
        frame: u64,
        read: impl FnOnce(&mut Reader<'_>) -> Result<Arc<T>, WireError>,
    ) -> Result<Arc<T>, WireError> {
        let tag = r.varint()?;
        let slot = usize::try_from(tag >> 1).map_err(|_| WireError::Invalid("slot"))?;
        if tag & 1 == 0 {
            let (value, used) = self
                .live
                .get_mut(slot)
                .and_then(Option::as_mut)
                .ok_or(WireError::UnknownSlot { slot })?;
            *used = frame;
            return Ok(Arc::clone(value));
        }
        // The encoder reuses a freed slot or takes the next new one, so a definition never lands further than one past the end.
        if slot > self.live.len() {
            return Err(WireError::UnknownSlot { slot });
        }
        let value = read(r)?;
        if slot == self.live.len() {
            self.live.push(None);
        }
        self.live[slot] = Some((Arc::clone(&value), frame));
        Ok(value)
    }

    pub(crate) fn sweep(&mut self, frame: u64) {
        for entry in &mut self.live {
            if entry.as_ref().is_some_and(|(_, used)| *used != frame) {
                *entry = None;
            }
        }
    }

    pub(crate) fn clear(&mut self) {
        self.live.clear();
    }
}
