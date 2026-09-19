//! [`Occurrences`]: the identity a reconcile gives each row, unique within one pass even when the source repeats a key.

use std::collections::HashMap;
use std::collections::hash_map::{DefaultHasher, Entry};
use std::hash::{Hash, Hasher};

/// Tells apart the items of one pass that name the same key: a repeated key would otherwise name several rows at once, leaking all but one on every pass, so each later occurrence gets a key derived from how many came before it.
pub(crate) struct Occurrences(HashMap<u64, u64>);

impl Occurrences {
    pub(crate) fn new() -> Self {
        Self(HashMap::new())
    }

    pub(crate) fn identify(&mut self, key: u64) -> u64 {
        let seen = self.0.entry(key).or_insert(0);
        let occurrence = *seen;
        *seen += 1;
        if occurrence == 0 {
            return key;
        }
        let mut h = DefaultHasher::new();
        (key, occurrence).hash(&mut h);
        h.finish()
    }
}

/// Indexes the previous pass's rows by key, handing back any row whose key another already took so the caller disposes it rather than losing track of it.
pub(crate) fn index_by_key<T>(
    keys: impl IntoIterator<Item = u64>,
    rows: impl IntoIterator<Item = T>,
) -> (HashMap<u64, T>, Vec<T>) {
    let mut indexed = HashMap::new();
    let mut strays = Vec::new();
    for (k, row) in keys.into_iter().zip(rows) {
        match indexed.entry(k) {
            Entry::Vacant(slot) => {
                slot.insert(row);
            }
            Entry::Occupied(_) => strays.push(row),
        }
    }
    (indexed, strays)
}
