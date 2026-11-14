use std::fmt;
use std::path::{Path, PathBuf};

/// `path` under `base`, one `/` between its components whatever the platform, or `None` when `path` is not under `base` or a component is not valid UTF-8.
pub fn relative_slashed(path: &Path, base: &Path) -> Option<String> {
    let relative = path.strip_prefix(base).ok()?;
    let parts: Option<Vec<&str>> = relative
        .components()
        .map(|part| part.as_os_str().to_str())
        .collect();
    Some(parts?.join("/"))
}

/// The components of `path` as text, for a module path or a `#[path]` built from them, or which leading part of `path` ends in the first component that is not valid UTF-8.
pub fn utf8_components(path: &Path) -> Result<Vec<&str>, NotUtf8Path> {
    path.components()
        .map(|part| part.as_os_str().to_str())
        .collect::<Option<_>>()
        .ok_or_else(|| not_utf8(path))
}

/// `path` as text, or which leading part of it ends in the first component that is not valid UTF-8.
pub(crate) fn utf8_path(path: &Path) -> Result<&str, NotUtf8Path> {
    path.to_str().ok_or_else(|| not_utf8(path))
}

fn not_utf8(path: &Path) -> NotUtf8Path {
    let mut leading = PathBuf::new();
    for part in path.components() {
        leading.push(part);
        if part.as_os_str().to_str().is_none() {
            break;
        }
    }
    NotUtf8Path { path: leading }
}

/// A file or directory generated Rust would have to name, whose name is not valid UTF-8: Rust source is UTF-8, so no `#[path]`, `include!` or module can spell it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotUtf8Path {
    /// The path up to and including the component that is not UTF-8.
    pub path: PathBuf,
}

impl fmt::Display for NotUtf8Path {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} is not named in UTF-8, so no Rust module can be declared through it: rename it in UTF-8",
            self.path.display()
        )
    }
}

impl std::error::Error for NotUtf8Path {}

#[cfg(test)]
#[path = "relative.test.rs"]
mod tests;
