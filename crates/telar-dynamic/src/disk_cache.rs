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
mod tests {
    use super::*;

    #[test]
    fn a_plain_id_is_its_own_file_name() {
        assert_eq!(file_name("arrow-right"), "arrow-right");
    }

    #[test]
    fn ids_that_differ_only_in_punctuation_do_not_share_a_file() {
        let colon = file_name("mdi:home");
        let slash = file_name("mdi/home");
        assert!(colon.starts_with("mdi_home_"), "{colon}");
        assert_ne!(colon, slash);
    }

    /// A separator in an id must not become a separator in the path, or the cache writes outside itself.
    #[test]
    fn an_id_cannot_escape_the_cache_directory() {
        let name = file_name("../../etc/passwd");
        assert!(!name.contains('/'), "{name}");
        assert!(!name.contains(".."), "{name}");
        assert_eq!(
            Path::new("/cache").join(&name).parent().unwrap(),
            Path::new("/cache")
        );
    }

    /// An empty id used to be harmless because the name always ended in `.svg`; with the extension gone it would name the kind directory itself.
    #[test]
    fn an_empty_id_still_names_a_file_and_not_its_directory() {
        let cache = DiskCache::new("/cache");
        let path = cache.path(&AssetKey::new("svg", ""));
        assert_eq!(path.parent().unwrap(), Path::new("/cache/svg"));
        assert!(path.file_name().is_some());
    }

    /// The collision the `kind` segment exists to prevent: the same id under two decoders is two files.
    #[test]
    fn two_formats_with_the_same_id_land_in_different_files() {
        let cache = DiskCache::new("/cache");
        let svg = cache.path(&AssetKey::new("svg", "logo"));
        let image = cache.path(&AssetKey::new("image", "logo"));
        assert_eq!(svg, Path::new("/cache/svg/logo"));
        assert_ne!(svg, image);
    }
}
