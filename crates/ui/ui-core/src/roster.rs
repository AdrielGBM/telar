//! [`Roster`]: entries kept in the order they joined and found by their key without a walk.

use std::hash::Hash;

use rustc_hash::FxHashMap;

/// Entries in the order they joined, each found, changed and withdrawn by its key in constant time: what a registry that a page of thousands of controls joins, reads and leaves one by one needs, where a list searched from the start costs every one of them the whole list.
///
/// A withdrawn entry leaves a hole that is closed once the holes outnumber the entries, so withdrawing costs no more than joining.
pub(crate) struct Roster<K, V> {
    slots: Vec<Option<(K, V)>>,
    at: FxHashMap<K, usize>,
}

impl<K, V> Default for Roster<K, V> {
    fn default() -> Self {
        Self {
            slots: Vec::new(),
            at: FxHashMap::default(),
        }
    }
}

impl<K: Copy + Eq + Hash, V> Roster<K, V> {
    /// Adds `value` under `key` after every entry there is, in place of one already under `key`.
    pub(crate) fn push(&mut self, key: K, value: V) {
        self.remove(key);
        self.at.insert(key, self.slots.len());
        self.slots.push(Some((key, value)));
    }

    pub(crate) fn get(&self, key: K) -> Option<&V> {
        let slot = self.slots.get(*self.at.get(&key)?)?;
        slot.as_ref().map(|(_, value)| value)
    }

    pub(crate) fn get_mut(&mut self, key: K) -> Option<&mut V> {
        let slot = self.slots.get_mut(*self.at.get(&key)?)?;
        slot.as_mut().map(|(_, value)| value)
    }

    pub(crate) fn contains(&self, key: K) -> bool {
        self.at.contains_key(&key)
    }

    /// Withdraws the entry under `key`, handing it back.
    pub(crate) fn remove(&mut self, key: K) -> Option<V> {
        let index = self.at.remove(&key)?;
        let (_, value) = self.slots[index].take()?;
        if self.slots.len() > 2 * self.at.len() + 32 {
            self.compact();
        }
        Some(value)
    }

    /// Withdraws every entry `leaves` picks, handing them back in the order they joined.
    pub(crate) fn remove_where(&mut self, leaves: impl Fn(&V) -> bool) -> Vec<V> {
        let keys: Vec<K> = self
            .iter_keyed()
            .filter(|(_, value)| leaves(value))
            .map(|(key, _)| key)
            .collect();
        keys.into_iter()
            .filter_map(|key| self.remove(key))
            .collect()
    }

    /// Withdraws every entry, handing them back in the order they joined.
    pub(crate) fn take_all(&mut self) -> Vec<V> {
        self.at.clear();
        std::mem::take(&mut self.slots)
            .into_iter()
            .flatten()
            .map(|(_, value)| value)
            .collect()
    }

    /// Every entry, in the order they joined.
    pub(crate) fn iter(&self) -> impl DoubleEndedIterator<Item = &V> {
        self.slots.iter().flatten().map(|(_, value)| value)
    }

    /// Every entry with its key, in the order they joined.
    pub(crate) fn iter_keyed(&self) -> impl Iterator<Item = (K, &V)> {
        self.slots
            .iter()
            .flatten()
            .map(|(key, value)| (*key, value))
    }

    fn compact(&mut self) {
        self.slots.retain(Option::is_some);
        self.at = self
            .slots
            .iter()
            .flatten()
            .enumerate()
            .map(|(index, (key, _))| (*key, index))
            .collect();
    }
}

#[cfg(test)]
#[path = "roster.test.rs"]
mod tests;
