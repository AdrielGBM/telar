//! Where an application's files go, asked rather than re-derived.
//!
//! [`AppPathsProvider`](crate::paths::AppPathsProvider) is the seam a platform adapter implements; this is the
//! side an *application* uses. The runner installs the platform's provider and the app's name once at startup,
//! so a caller asks `paths::cache()` instead of resolving `$XDG_CACHE_HOME` for itself and getting a different
//! answer than the runtime it is embedded in.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::paths::AppPathsProvider;

struct Installed {
    name: String,
    provider: std::sync::Arc<dyn AppPathsProvider>,
}

static INSTALLED: OnceLock<Installed> = OnceLock::new();

/// Installs the platform's provider and the name every app-scoped directory is nested under. Called by the
/// runner; a second call is ignored, so an embedded surface cannot repoint a host's directories.
pub fn install(name: impl Into<String>, provider: std::sync::Arc<dyn AppPathsProvider>) {
    let _ = INSTALLED.set(Installed {
        name: name.into(),
        provider,
    });
}

fn scoped(base: impl Fn(&dyn AppPathsProvider) -> Option<PathBuf>) -> Option<PathBuf> {
    let installed = INSTALLED.get()?;
    Some(base(installed.provider.as_ref())?.join(&installed.name))
}

/// The app's own directory under the platform's config root, or `None` before a runner has installed one —
/// which is also what a preview window and a headless test get, so neither touches a real XDG path.
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

/// The user's home directory, or `None` where `$HOME` names nothing — which is how a process started without an
/// environment presents, and a reason to fall back rather than to build a path rooted at `/`.
pub fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
}

/// Expands a leading `~` (bare or `~/…`) to `$HOME`, leaving every other path untouched.
///
/// User-authored config paths commonly use `~`, which the OS does not resolve on its own.
pub fn expand_tilde(path: &Path) -> PathBuf {
    let Ok(rest) = path.strip_prefix("~") else {
        return path.to_path_buf();
    };
    match home() {
        Some(home) => home.join(rest),
        None => path.to_path_buf(),
    }
}

/// Creates `dir` (and its parents) and returns it, so a caller can chain straight into a file path.
pub fn ensure_dir(dir: PathBuf) -> PathBuf {
    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::warn!("could not create {}: {e}", dir.display());
    }
    dir
}

/// A well-known user directory (`XDG_PICTURES_DIR`, `XDG_VIDEOS_DIR`, …), else `$HOME/<fallback>`.
///
/// These are not environment variables on most sessions: `xdg-user-dirs` writes them to
/// `user-dirs.dirs` in the config root as a shell fragment that a login script sources, so a process started
/// any other way never sees them. Reading the file directly is what makes a screenshot land in the user's own
/// pictures directory on a localised system, where it is called `Imágenes` and no fallback would find it.
pub fn user_dir(name: &str, fallback: &str) -> PathBuf {
    let home = home();
    let default = || match &home {
        Some(home) => home.join(fallback),
        None => PathBuf::from(fallback),
    };
    if let Some(value) = std::env::var_os(name).filter(|v| !v.is_empty()) {
        return PathBuf::from(value);
    }
    let Some(home) = home.clone() else {
        return default();
    };
    let Some(config_root) = INSTALLED
        .get()
        .and_then(|i| i.provider.config_dir())
        .or_else(|| {
            std::env::var_os("XDG_CONFIG_HOME")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
                .or_else(|| Some(home.join(".config")))
        })
    else {
        return default();
    };
    let Ok(text) = std::fs::read_to_string(config_root.join("user-dirs.dirs")) else {
        return default();
    };
    parse_user_dirs(&text, name)
        .map(|value| PathBuf::from(value.replace("$HOME", &home.to_string_lossy())))
        .unwrap_or_else(default)
}

/// Reads one `NAME="value"` assignment out of `user-dirs.dirs`, ignoring comments. `$HOME` is left in the value
/// for the caller to expand, since only it knows what home is.
fn parse_user_dirs(text: &str, name: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != name {
            continue;
        }
        let value = value.trim().trim_matches('"');
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }
    None
}

/// The XDG resolution rule over values rather than the environment: the variable when it names a non-empty
/// path, else `$HOME` joined with `fallback`, else `fallback` relative.
///
/// Taking the values as arguments keeps it testable without mutating process-wide environment, which would race
/// every other test in the binary.
pub fn resolve_base(var: Option<OsString>, home: Option<OsString>, fallback: &str) -> PathBuf {
    var.map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .or_else(|| home.map(|h| PathBuf::from(h).join(fallback)))
        .unwrap_or_else(|| PathBuf::from(fallback))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_leading_tilde_becomes_home_and_nothing_else_moves() {
        let absolute = Path::new("/etc/hosts");
        assert_eq!(expand_tilde(absolute), absolute);
        let relative = Path::new("pictures/a.png");
        assert_eq!(expand_tilde(relative), relative);
        // A `~` inside the path is a directory literally called `~`, not a home to expand.
        let inner = Path::new("/tmp/~/x");
        assert_eq!(expand_tilde(inner), inner);
    }

    #[test]
    fn a_user_dirs_assignment_is_read_past_its_quotes_and_comments() {
        let text =
            "# generated\nXDG_PICTURES_DIR=\"$HOME/Imágenes\"\nXDG_VIDEOS_DIR=\"$HOME/Vídeos\"\n";
        assert_eq!(
            parse_user_dirs(text, "XDG_PICTURES_DIR").as_deref(),
            Some("$HOME/Imágenes")
        );
        assert_eq!(parse_user_dirs(text, "XDG_MUSIC_DIR"), None);
    }

    #[test]
    fn a_commented_assignment_is_not_an_answer() {
        let text = "#XDG_PICTURES_DIR=\"$HOME/wrong\"\nXDG_PICTURES_DIR=\"$HOME/right\"\n";
        assert_eq!(
            parse_user_dirs(text, "XDG_PICTURES_DIR").as_deref(),
            Some("$HOME/right")
        );
    }

    #[test]
    fn the_xdg_rule_prefers_the_variable_then_home_then_the_bare_fallback() {
        assert_eq!(
            resolve_base(Some("/x".into()), Some("/home/u".into()), ".cache"),
            PathBuf::from("/x")
        );
        // An empty variable is not an answer: it is how an unset one presents through the shell.
        assert_eq!(
            resolve_base(Some("".into()), Some("/home/u".into()), ".cache"),
            PathBuf::from("/home/u/.cache")
        );
        assert_eq!(resolve_base(None, None, ".cache"), PathBuf::from(".cache"));
    }

    /// Nothing installed is the preview/headless case, and it must answer `None` rather than guess a real path.
    #[test]
    fn an_app_directory_is_none_until_a_runner_installs_one() {
        if INSTALLED.get().is_none() {
            assert_eq!(cache(), None);
            assert_eq!(runtime(), None);
        }
    }
}
