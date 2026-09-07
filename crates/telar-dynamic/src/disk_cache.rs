//! Keeping resolved asset bytes in a directory, so a restart resolves from disk instead of the network.

use std::path::{Path, PathBuf};

use ui_core::{AssetCache, AssetKey};

/// An [`AssetCache`] over a directory, laid out as `<root>/<kind>/<name>`.
///
/// The `kind` segment is not decoration: an SVG named `logo` and a bitmap named `logo` are different assets, and a cache keyed on the id alone would let one overwrite the other's file with bytes the wrong decoder then chokes on.
///
/// Nothing here re-checks what it is given. A loader reaches [`AssetCache::put`] only once the bytes have already decoded, so a provider's 404 page — a 200 with HTML in it, often enough — has failed before it can get this far and make one typo permanent.
///
/// Create it under a directory the platform means for discardable data, which on a Telar application is `telar::paths::cache()`. The directories are created on first write.
pub struct DiskCache {
    root: PathBuf,
}

impl DiskCache {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The directory this cache writes under.
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn path(&self, key: &AssetKey) -> PathBuf {
        self.root.join(file_name(key.kind)).join(file_name(&key.id))
    }
}

impl AssetCache for DiskCache {
    fn get(&self, key: &AssetKey) -> Option<Vec<u8>> {
        std::fs::read(self.path(key)).ok()
    }

    fn put(&self, key: &AssetKey, bytes: &[u8]) {
        let path = self.path(key);
        let Some(dir) = path.parent() else {
            return;
        };
        if let Err(e) = std::fs::create_dir_all(dir) {
            tracing::warn!("could not create asset cache {}: {e}", dir.display());
            return;
        }
        if let Err(e) = std::fs::write(&path, bytes) {
            tracing::warn!("could not cache asset {}: {e}", path.display());
        }
    }
}

/// A path segment that is a path segment: everything outside `[A-Za-z0-9_-]` becomes `_`, and anything that was not already such a name carries a hash of the original so two names cannot collapse onto one file.
///
/// The hash is not decoration. Ids arrive from application config (`mdi:home`, `lucide/arrow-right`), so without it `a:b` and `a/b` would share a file, and `../../x` would name a path outside the cache at all. Kinds arrive from a decoder rather than from config, but they land in the same position and go through the same rule, so a careless third-party [`ui_core::AssetDecoder`] cannot write outside the root either.
fn file_name(name: &str) -> String {
    let simple = !name.is_empty()
        && name.len() <= 32
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if simple {
        return name.to_string();
    }

    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    name.hash(&mut hasher);
    let hash = hasher.finish();

    let sanitized: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .take(32)
        .collect();
    format!("{sanitized}_{hash:x}")
}

#[cfg(test)]
#[path = "disk_cache_test.rs"]
mod tests;
