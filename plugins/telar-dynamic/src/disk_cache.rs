//! Keeping resolved asset bytes in a directory, so a restart resolves from disk instead of the network.

use std::path::{Path, PathBuf};

use telar::{AssetCache, AssetKey};

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
    /// A cache that keeps its entries under `root`, which nothing creates until the first write.
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
        if let Err(e) = write_whole(&path, bytes) {
            tracing::warn!("could not cache asset {}: {e}", path.display());
        }
    }
}

/// A reader finds the previous entry or the new one, never part of one: `get` is trusted without a fetch, so a truncated entry that still decodes would be kept for good.
fn write_whole(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let dir = path.parent().unwrap_or(Path::new(""));
    std::fs::create_dir_all(dir)?;
    let mut file = tempfile::NamedTempFile::new_in(dir)?;
    file.write_all(bytes)?;
    rename_over(file.into_temp_path(), path)
}

// Renamed by `std::fs::rename` rather than the temporary file's own persist: on Windows only the former replaces a file another handle has open, a reader or a concurrent writer's rename, where the latter is denied access.
fn rename_over(temp: tempfile::TempPath, path: &Path) -> std::io::Result<()> {
    let temp = temp.keep()?;
    std::fs::rename(&temp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&temp);
    })
}

/// The hash over the original name keeps case-distinct ids apart on case-insensitive file systems, keeps every segment off Windows device names, and stops a crafted id from landing on another id's file.
fn file_name(name: &str) -> String {
    use sha2::{Digest, Sha256};
    use std::fmt::Write;

    let mut file_name: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .take(16)
        .collect();
    if file_name.is_empty() {
        file_name.push('_');
    }
    file_name.push('-');
    for byte in &Sha256::digest(name.as_bytes())[..16] {
        write!(file_name, "{byte:02x}").expect("writing to a String cannot fail");
    }
    file_name
}

#[cfg(test)]
#[path = "disk_cache.test.rs"]
mod tests;
