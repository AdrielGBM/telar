//! Keeping resolved asset bytes in this process, so a second reader of the same id does not repeat the transport.

use std::sync::Mutex;

use renderer_cache::{Cache, Policy};
use ui_core::{AssetCache, AssetKey};

/// What [`MemoryCache::new`] budgets when nothing says otherwise.
///
/// These are *encoded* bytes — the SVG document, the PNG file — not the decoded image, so the number that matters is the size of the source files rather than of the pixels they become. A few thousand icons at a couple of kilobytes each fit inside this with room to spare, which is the shape this cache is for; an application resolving photographs should say so with [`MemoryCache::new`] and pick its own.
pub const DEFAULT_BUDGET_BYTES: usize = 8 * 1024 * 1024;

/// An [`AssetCache`] that keeps bytes in memory, bounded by a byte budget and evicting least-recently-used first.
///
/// The counterpart to `DiskCache`, and the one that works everywhere: a browser has no directory to write to, so a build that reached for the disk cache there resolved every id over the network on every run while reporting nothing. This one has no such target — and no persistence either, which is the whole of the difference. A restart resolves from the transport again.
///
/// Both at once is the usual arrangement and the seam does not offer it: [`AssetCache`] is one role, so a loader takes one cache. Chaining them is a cache of your own that holds both and asks the fast one first.
///
/// Nothing here re-checks what it is given, for the same reason `DiskCache` does not: a loader reaches [`AssetCache::put`] only once the bytes have already decoded, so a provider's 404 page has failed before it can get this far.
///
/// ```ignore
/// let icons = AssetLoader::new(
///     Arc::new(HttpTransport::new("https://api.iconify.design/{name}.svg")),
///     Some(Arc::new(MemoryCache::default())),
///     Arc::new(SvgDecoder),
/// );
/// ```
pub struct MemoryCache {
    // `Mutex` and not `RefCell`: `AssetCache` is `Send + Sync`, because one cache is shared by every loader reading through it and a transport replies from whatever thread ended up holding the bytes. Contention is not a concern — the critical section is a hash lookup and a memcpy, and the alternative was a cache that could not be installed at all.
    entries: Mutex<Cache<AssetKey, Vec<u8>>>,
}

impl MemoryCache {
    /// A cache holding at most `capacity_bytes` of encoded assets.
    pub fn new(capacity_bytes: usize) -> Self {
        Self::with_policy(Policy::new(capacity_bytes))
    }

    /// The same, under a policy of your own — an idle horizon, or admission on second use.
    ///
    /// Neither is on by default, and the reason is what an asset costs to lose. A renderer's caches evict by age because re-rasterizing a glyph is microseconds; here the entry that ages out is one HTTP round trip, and a horizon that guesses wrong pays it again over the network. The byte budget bounds the cache on its own, and a build that would rather bound it by age as well can say so.
    pub fn with_policy(policy: Policy) -> Self {
        Self {
            entries: Mutex::new(Cache::new(policy, Vec::len)),
        }
    }

    /// What the cache is holding, for an application answering "where did the memory go" from outside it.
    pub fn stat(&self) -> renderer_cache::CacheStat {
        self.locked().stat("assets")
    }

    /// Drops everything, for a locale switch or a sign-out that invalidates what was resolved under the old one.
    pub fn clear(&self) {
        self.locked().clear();
    }

    /// A poisoned lock means a panic while the cache was borrowed, which for a byte cache is recoverable: the map is a plain container and nothing here holds an invariant across the panic. Taking the value back is better than propagating a panic into whichever frame happened to ask next.
    fn locked(&self) -> std::sync::MutexGuard<'_, Cache<AssetKey, Vec<u8>>> {
        self.entries.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Default for MemoryCache {
    fn default() -> Self {
        Self::new(DEFAULT_BUDGET_BYTES)
    }
}

impl AssetCache for MemoryCache {
    fn get(&self, key: &AssetKey) -> Option<Vec<u8>> {
        self.locked().get(key).cloned()
    }

    fn put(&self, key: &AssetKey, bytes: &[u8]) {
        self.locked().insert(key.clone(), bytes.to_vec());
    }
}

#[cfg(test)]
#[path = "memory_cache_test.rs"]
mod tests;
