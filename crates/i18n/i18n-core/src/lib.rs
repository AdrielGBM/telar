//! Runtime for rsx internationalization: the baked message model, the reactive active-locale signal, and the `translate` lookup that binds them.
//!
//! This crate is always-on and dependency-light (only `reactive-core`). The heavy work — parsing translation catalogs — happens at build time in the transpiler's i18n baker, which emits a `static CATALOG: Catalog` of pure `&'static` data. At runtime a translated string is nothing more than `translate(&CATALOG, key, args)`, which reads the active locale reactively and renders the matching [`Message`].

#![warn(rustdoc::broken_intra_doc_links)]

#[cfg(feature = "runtime-catalog")]
mod catalog;
mod installed;
mod locale;
mod message;
mod negotiate;
mod plural;

#[cfg(feature = "runtime-catalog")]
pub use catalog::{CatalogModel, MessageModel, PartModel, flatten, is_plural_table, parse_message};
pub use installed::{catalog, set_catalog, t};
pub use locale::{current_locale, set_locale, use_locale};
pub use message::{Catalog, Entry, Message, Part};
pub use negotiate::negotiate_locale;
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

/// Translates `key` from a plugin's own `catalog`, letting the app's installed catalog override it under `"<namespace>.<key>"`.
///
/// An opt-in crate ships its strings ("Close", "Select") in its own catalog, but the process has one installed catalog, the app's. The app overrides or translates a plugin string by adding `"<namespace>.<key>"` to its catalog; otherwise the plugin's text is used. Reads the locale reactively like [`translate`], so a widget calling this in its content closure re-renders on a language switch.
///
/// Resolution order, first hit wins: the app's message for the active locale, the plugin's message for the active locale, the app's default-locale message, the plugin's default-locale message, then the raw `key`. An active-locale message always beats a default-locale one, so an app that overrides only its default language never shows that text to a user whose language the plugin already translates. When no locale is set, the app's default locale is the active one.
///
/// With no installed catalog this is exactly `translate(catalog, key, args)`.
///
/// Like [`set_catalog`], the `&Catalog` must outlive every call: a catalog baked into a **hot-reload dylib** lives in that dylib's data, so a reference kept across a reload dangles. A plugin should hand over a catalog it loaded at runtime, or take a fresh reference after each reload.
pub fn translate_with_override(
    namespace: &str,
    catalog: &Catalog,
    key: &str,
    args: &[(&str, &str)],
) -> String {
    let Some(app) = installed::catalog() else {
        return translate(catalog, key, args);
    };
    let active = use_locale();
    let locale = active.as_deref().unwrap_or(app.default_locale);
    let namespaced = format!("{namespace}.{key}");
    app.message_in(&namespaced, locale)
        .or_else(|| catalog.message_in(key, locale))
        .or_else(|| app.message_in(&namespaced, app.default_locale))
        .or_else(|| catalog.message_in(key, catalog.default_locale))
        .map(|m| m.select(locale, args).render(args))
        .unwrap_or_else(|| key.to_string())
}

#[cfg(test)]
#[path = "lib_test.rs"]
mod tests;
