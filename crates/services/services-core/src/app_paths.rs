//! Where an application's files go, asked rather than re-derived.
//!
//! [`AppPathsProvider`] is the seam a platform adapter implements; this is the side an *application* uses. The runner installs the platform's provider and the app's name once at startup, so a caller asks `paths::cache()` instead of resolving `$XDG_CACHE_HOME` for itself and getting a different answer than the runtime it is embedded in.

use std::path::PathBuf;
use std::sync::Arc;

use crate::paths::AppPathsProvider;

#[derive(Clone)]
pub(crate) struct Installed {
    pub(crate) name: String,
    pub(crate) provider: Arc<dyn AppPathsProvider>,
}

reactive_local::released_slot! {
    static INSTALLED: Installed;
}

/// Installs the platform's provider and the name every app-scoped directory is nested under. Called by the runner; a second call is ignored, so an embedded surface cannot repoint a host's directories.
pub fn install(name: impl Into<String>, provider: Arc<dyn AppPathsProvider>) {
    INSTALLED.set_first(Installed {
        name: name.into(),
        provider,
    });
}

pub(crate) fn installed() -> Option<Installed> {
    INSTALLED.get()
}

/// The name the runner installed, or `None` before one has.
pub fn name() -> Option<String> {
    installed().map(|installed| installed.name)
}

fn scoped(base: impl Fn(&dyn AppPathsProvider) -> Option<PathBuf>) -> Option<PathBuf> {
    let installed = installed()?;
    Some(base(installed.provider.as_ref())?.join(&installed.name))
}

/// The app's own directory under the platform's config root, or `None` before a runner has installed one — which is also what a preview window and a headless test get, so neither touches a real XDG path.
pub fn config() -> Option<PathBuf> {
    scoped(|p| p.config_dir())
}

/// Persistent user data the app owns.
pub fn data() -> Option<PathBuf> {
    scoped(|p| p.data_dir())
}

/// Regenerable artefacts — thumbnails, decoded icons — that are safe to delete.
pub fn cache() -> Option<PathBuf> {
    scoped(|p| p.cache_dir())
}

/// Machine-written state the user never edits, as opposed to the config they own.
pub fn state() -> Option<PathBuf> {
    scoped(|p| p.state_dir())
}

/// Session-scoped runtime files — a socket, a lock — on a platform that has such a place for them.
pub fn runtime() -> Option<PathBuf> {
    scoped(|p| p.runtime_dir())
}

/// The user's home directory, or `None` where `$HOME` names nothing — which is how a process started without an environment presents, and a reason to fall back rather than to build a path rooted at `/`.
pub fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
}

#[cfg(test)]
#[path = "app_paths.test.rs"]
mod tests;
