//! Reading asset bytes from a directory on disk.

use std::path::{Component, Path, PathBuf};

use ui_core::{AssetError, AssetKey, AssetTransport, Reply};

/// An [`AssetTransport`] that resolves each id against a directory.
///
/// Unlike [`crate::DiskCache`], which flattens an id into one file name because it owns its own storage, this keeps the id's shape: `"icons/mdi/home.svg"` is `root/icons/mdi/home.svg`. A directory of content is a tree, and reading it as anything else would make half of one unreachable.
///
/// **`root` is the application's decision, not the library's**, because only the application knows whether the files ship with it or arrive afterwards. The two real answers are [`telar::paths::data()`](https://docs.rs/telar) — the per-user data directory, for content downloaded or authored after install — and a directory beside `std::env::current_exe()`, for content shipped alongside the binary. A relative path is neither: it resolves against the working directory, which is wherever the user happened to launch from.
///
/// ```ignore
/// let transport = DirTransport::new(telar::paths::data().join("icons"));
/// let icons = AssetLoader::new(Arc::new(transport), None, Arc::new(SvgDecoder));
/// ```
pub struct DirTransport {
    root: PathBuf,
}

impl DirTransport {
    /// A transport that reads from `root`. The directory is not required to exist yet — one that appears later starts resolving without a restart, and until then every id fails the way a missing file does.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn read(&self, id: &str) -> Result<Vec<u8>, AssetError> {
        let path = resolve(&self.root, id)?;
        std::fs::read(&path).map_err(|e| AssetError(format!("{}: {e}", path.display())))
    }
}

impl AssetTransport for DirTransport {
    /// Blocking, like the native HTTP transport and for the same reason: the loader runs this on a job in `reactive_core`'s elastic pool, so a read that waits parks one pooled thread and nothing else.
    fn load(&self, key: &AssetKey, reply: Reply) {
        reply(self.read(&key.id));
    }
}

/// `root`-relative `id` as an absolute path, or an error if it would leave `root`.
///
/// Two layers, because neither catches what the other does. The first rejects the id's *spelling* — a `..`, an absolute path, a Windows volume prefix — before touching the filesystem, which is what stops `../../etc/passwd` and costs nothing. The second canonicalizes and checks containment, which is the only thing that catches a symlink inside `root` pointing out of it: that id is spelled innocently and only the resolved path gives it away.
fn resolve(root: &Path, id: &str) -> Result<PathBuf, AssetError> {
    let relative = Path::new(id);
    for component in relative.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            _ => {
                return Err(AssetError(format!(
                    "asset id `{id}` is not a path inside the asset directory"
                )));
            }
        }
    }

    let candidate = root.join(relative);
    // A path that cannot be canonicalized does not exist, and reporting that is the caller's read to do — but it must not be handed back as if containment had been checked, so it fails here.
    let resolved = candidate
        .canonicalize()
        .map_err(|e| AssetError(format!("{}: {e}", candidate.display())))?;
    let root = root
        .canonicalize()
        .map_err(|e| AssetError(format!("{}: {e}", root.display())))?;
    if !resolved.starts_with(&root) {
        return Err(AssetError(format!(
            "asset id `{id}` resolves outside the asset directory"
        )));
    }
    Ok(resolved)
}

#[cfg(test)]
#[path = "dir_test.rs"]
mod tests;
