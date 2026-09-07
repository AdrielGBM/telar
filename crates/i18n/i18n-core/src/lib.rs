//! Runtime for rsx internationalization: the baked message model, the reactive active-locale signal, and the `translate` lookup that binds them.
//!
//! This crate is always-on and dependency-light (only `reactive-core`). The heavy work — parsing translation catalogs — happens at build time in the transpiler's i18n baker, which emits a `static CATALOG: Catalog` of pure `&'static` data. At runtime a translated string is nothing more than `translate(&CATALOG, key, args)`, which reads the active locale reactively and renders the matching [`Message`].

#![warn(rustdoc::broken_intra_doc_links)]

#[cfg(feature = "runtime-catalog")]
mod catalog;
mod installed;
mod locale;
mod message;
mod plural;

#[cfg(feature = "runtime-catalog")]
pub use catalog::{CatalogModel, MessageModel, PartModel, flatten, is_plural_table, parse_message};
pub use installed::{catalog, set_catalog, t};
pub use locale::{current_locale, detect_system_locale, set_locale, use_locale};
pub use message::{Catalog, Entry, Message, Part};
pub use plural::{PluralCategory, plural_category};

/// Looks up `key` in `catalog` for the currently active locale and renders it with `args`.
///
/// The active locale is read reactively via [`use_locale`], so calling this inside a widget's `Fn() -> String` content closure subscribes that widget to locale changes — a language switch re-renders it automatically. Falls back to the catalog's default locale when no locale is set or the active one lacks the key, and to the raw `key` when the key is absent entirely (which the build-time validator normally prevents).
pub fn translate(catalog: &Catalog, key: &str, args: &[(&str, &str)]) -> String {
    let active = use_locale();
    let locale = active.as_deref().unwrap_or(catalog.default_locale);
    catalog
        .message(key, locale)
        .map(|m| m.select(locale, args).render(args))
        .unwrap_or_else(|| key.to_string())
}

#[cfg(test)]
#[path = "lib_test.rs"]
mod tests;
