//! The catalog reached without naming one.
//!
//! `t!` resolves its catalog at expansion time, by path, into the module the transpiler wrote — which only exists where the transpiler ran. Plain Rust needs the other half: one catalog installed for the process, and a lookup that finds it. That is the whole of this module.

use std::sync::RwLock;

use crate::message::Catalog;

/// A `&'static Catalog` is immutable `&'static` data throughout, so sharing it across threads costs nothing beyond the lock this replaces it under.
static INSTALLED: RwLock<Option<&'static Catalog>> = RwLock::new(None);

/// Installs the catalog [`t`] looks in. Process-wide and replaceable — a language pack loaded later takes over from here on.
///
/// Takes `&'static` because that is what a catalog is on both paths: a `static CATALOG` the transpiler baked, or the heap-leaked result of [`Catalog::from_dir`](crate::Catalog::from_dir). One caveat, and only one: a catalog baked into a **hot-reload dylib** lives in that dylib's data, so installing it and then reloading leaves this pointing at unmapped memory. Install a runtime-loaded catalog there, or re-install after each reload.
pub fn set_catalog(catalog: &'static Catalog) {
    *INSTALLED.write().unwrap_or_else(|e| e.into_inner()) = Some(catalog);
}

/// The installed catalog, or `None` before [`set_catalog`].
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
mod tests {
    use super::*;
    use crate::message::{Entry, Message, Part};

    static CATALOG: Catalog = Catalog {
        locales: &["en", "es"],
        default_locale: "en",
        entries: &[Entry {
            key: "greeting",
            messages: &[
                (
                    "en",
                    Message::Format(&[Part::Lit("Hello, "), Part::Arg("name"), Part::Lit("!")]),
                ),
                (
                    "es",
                    Message::Format(&[Part::Lit("Hola, "), Part::Arg("name"), Part::Lit("!")]),
                ),
            ],
        }],
    };

    #[test]
    fn an_installed_catalog_follows_the_active_locale() {
        set_catalog(&CATALOG);
        crate::set_locale("es");
        assert_eq!(t("greeting", &[("name", "Ada")]), "Hola, Ada!");
        crate::set_locale("en");
        assert_eq!(t("greeting", &[("name", "Ada")]), "Hello, Ada!");
    }

    /// A missing key must reach the screen as itself. Returning an empty string would hide the gap, which is the one thing a translation lookup must not do.
    #[test]
    fn an_absent_key_renders_as_the_key() {
        set_catalog(&CATALOG);
        assert_eq!(t("nope", &[]), "nope");
    }
}
