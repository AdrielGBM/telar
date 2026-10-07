//! The catalog reached without naming one.
//!
//! `t!` resolves its catalog at expansion time, by path, into the module the transpiler wrote — which only exists where the transpiler ran. Plain Rust needs the other half: one catalog installed for the process, and a lookup that finds it. That is the whole of this module.

use std::sync::RwLock;

use crate::message::Catalog;

/// A `&'static Catalog` is immutable `&'static` data throughout, so sharing it across threads costs nothing beyond the lock this replaces it under.
static INSTALLED: RwLock<Option<&'static Catalog>> = RwLock::new(None);

/// Installs the catalog [`t`] and [`translate_with_override`](crate::translate_with_override) look in. Process-wide and replaceable — a language pack loaded later takes over from here on.
///
/// An application does not call this for the catalog it bakes from `locales/`: `telar::app!` installs that one as the binary loads, before `main` or any test runs. This is for replacing it while the application runs.
///
/// Takes `&'static` because that is what a catalog is on both paths: a `static CATALOG` the transpiler baked, or the heap-leaked result of `Catalog::from_dir` (feature `runtime-catalog`).
///
/// The slot belongs to the copy of this crate that holds it. A **hot-reload dylib** links a copy of its own, apart from its host's, and every load of the dylib installs its baked catalog into that copy before anything in it runs, so its widgets look in a catalog that stays mapped exactly as long as they do. Only a dylib's catalog handed across that boundary, into its host's copy, would outlive its data.
pub fn set_catalog(catalog: &'static Catalog) {
    *INSTALLED.write().unwrap_or_else(|e| e.into_inner()) = Some(catalog);
}

/// Installs `catalog` unless one already is, answering whether it did.
///
/// What `telar::rsx_modules!` does with the catalog of an application crate as its binary loads: an application that starts its own runner, or a test that mounts its components, gets the catalog without a call, while the one `telar::app!` installs wins whichever of the two loads first.
pub fn set_catalog_if_unset(catalog: &'static Catalog) -> bool {
    let mut installed = INSTALLED.write().unwrap_or_else(|e| e.into_inner());
    if installed.is_some() {
        return false;
    }
    *installed = Some(catalog);
    true
}

/// The installed catalog, or `None` where nothing installed one: a binary with no application catalog baked into it and no [`set_catalog`] call.
pub fn catalog() -> Option<&'static Catalog> {
    *INSTALLED.read().unwrap_or_else(|e| e.into_inner())
}

/// Translates `key` against the installed catalog for the active locale.
///
/// The runtime twin of the `t!` macro, for code the transpiler never sees — a plain Rust module, a crate with no `.rsx` file in it at all. Same catalog format, same lookup, same reactive read of the locale: calling this inside a widget's content closure subscribes that widget to language switches.
///
/// It is a *function*, so it cannot check the key at build time the way `t!` does. An absent key — or no catalog installed yet — renders as the key itself, which is visible in the UI rather than silent.
///
/// ```
/// # use telar_i18n_core::{Catalog, Entry, Message, set_catalog, set_locale, t};
/// # static CATALOG: Catalog = Catalog {
/// #     locales: &["en"],
/// #     default_locale: "en",
/// #     entries: &[Entry { key: "greeting", messages: &[("en", Message::Plain("Hello"))] }],
/// # };
/// set_catalog(&CATALOG);
/// set_locale("en");
/// assert_eq!(t("greeting", &[]), "Hello");
/// assert_eq!(t("absent", &[]), "absent");
/// ```
pub fn t(key: &str, args: &[(&str, &str)]) -> String {
    match catalog() {
        Some(catalog) => crate::translate(catalog, key, args),
        None => key.to_string(),
    }
}

#[cfg(test)]
#[path = "installed_test.rs"]
mod tests;
