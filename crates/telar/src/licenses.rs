//! The third-party licence notices an application's build compiled into it, for the screen that shows them.
//!
//! The build writes the notice of the icons an application bakes beside it, but a bundle cannot always carry a file: an APK is signed before the notice is known, and a browser has no directory to read it from. So `telar::app!` and `telar::rsx_modules!` also compile the text into the binary and install it here as the binary loads, and a plugin reads it back without the application passing anything along.

use std::sync::RwLock;

static ICON_LICENSES: RwLock<Option<&'static str>> = RwLock::new(None);

/// Installs the icon licence notice an expansion compiled in. Replaceable, because the slot belongs to the image that holds it: a hot-reload dylib links a copy of the facade of its own, and every load of it installs the notice it carries into that copy.
#[doc(hidden)]
pub fn install_icon_licenses(notice: &'static str) {
    *ICON_LICENSES.write().unwrap_or_else(|e| e.into_inner()) = Some(notice);
}

/// The licence notice of the icons this application baked, and of the libraries it is built with that baked any; `None` when neither did. `telar-icons` exposes it as `telar_icons::licenses()`.
pub fn icon_licenses() -> Option<&'static str> {
    *ICON_LICENSES.read().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
#[path = "licenses_test.rs"]
mod tests;
