use std::path::Path;

/// `path` under `base`, one `/` between its components whatever the platform, or `None` when `path` is not under `base` or a component is not valid UTF-8.
pub fn relative_slashed(path: &Path, base: &Path) -> Option<String> {
    let relative = path.strip_prefix(base).ok()?;
    let parts: Option<Vec<&str>> = relative
        .components()
        .map(|part| part.as_os_str().to_str())
        .collect();
    Some(parts?.join("/"))
}

#[cfg(test)]
#[path = "relative.test.rs"]
mod tests;
