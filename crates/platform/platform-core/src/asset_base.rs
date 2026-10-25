//! Where the files a build ships beside the app are addressed from.

use std::sync::Arc;

reactive_local::released_slot! {
    static BASE: Arc<str>;
}

/// Installs the address the build's files live under, ending in `/`. The web platform installs it once at start, before the app moves the address bar: a relative path resolved later would resolve against wherever the app had navigated to.
pub fn set_asset_base(base: &str) {
    let base = if base.ends_with('/') {
        Arc::from(base)
    } else {
        Arc::from(format!("{base}/"))
    };
    BASE.replace(base);
}

/// The address [`set_asset_base`] installed, ending in `/`, or `None` where no base was installed.
pub fn asset_base() -> Option<String> {
    BASE.get().map(|base| base.to_string())
}

/// `path`, a file the build shipped, as an address to fetch it from; unchanged where no base was installed.
pub fn asset_url(path: &str) -> String {
    match BASE.get() {
        Some(base) => format!("{base}{}", path.trim_start_matches("./")),
        None => path.to_string(),
    }
}

#[cfg(test)]
#[path = "asset_base_test.rs"]
mod tests;
