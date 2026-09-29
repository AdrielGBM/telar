//! Where the files a build ships beside the app are addressed from.

use std::sync::{Arc, RwLock};

static BASE: RwLock<Option<Arc<str>>> = RwLock::new(None);

/// Installs the address the build's files live under, ending in `/`. The web platform installs it once at start, before the app moves the address bar: a relative path resolved later would resolve against wherever the app had navigated to.
pub fn set_asset_base(base: &str) {
    let base = if base.ends_with('/') {
        Arc::from(base)
    } else {
        Arc::from(format!("{base}/"))
    };
    *BASE.write().unwrap_or_else(|e| e.into_inner()) = Some(base);
}

/// `path`, a file the build shipped, as an address to fetch it from; unchanged where no base was installed.
pub fn asset_url(path: &str) -> String {
    match BASE.read().unwrap_or_else(|e| e.into_inner()).as_deref() {
        Some(base) => format!("{base}{}", path.trim_start_matches("./")),
        None => path.to_string(),
    }
}

#[cfg(test)]
#[path = "asset_base_test.rs"]
mod tests;
