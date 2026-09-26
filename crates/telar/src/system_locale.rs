//! Binds the active locale to the user's preferred locales.
//!
//! In the facade for the same reason as `direction`: `i18n-core` stays on `reactive-core` alone, and `preferences-core` knows nothing of translations.

use std::cell::Cell;

thread_local! {
    // An `Effect` handle is inert, so the effect gets a scope of its own that a re-call disposes; otherwise two followers would race to set the locale.
    static FOLLOW: Cell<Option<reactive_core::OwnerId>> = const { Cell::new(None) };
}

/// Makes the active locale follow the user's preferred locales, negotiated against `available` with [`negotiate_locale`](crate::negotiate_locale) — so a user who reads Chilean Spanish first gets the `es` catalog — and re-negotiated live when that list changes. Re-calling replaces the previous follower.
///
/// Call once at app start, in place of an initial [`set_locale`](crate::set_locale). An empty list is not a vote for `fallback`: it leaves an active locale alone, and selects `fallback` only when none is set yet. A manual [`set_locale`](crate::set_locale) still wins until the list changes.
///
/// An application whose locale is part of where the user is (a URL prefix, a saved setting) owns that choice instead, and calls [`negotiate_locale`](crate::negotiate_locale) itself only when nothing chose one yet.
pub fn follow_system_locale(
    available: impl IntoIterator<Item = impl Into<String>>,
    fallback: impl Into<String>,
) {
    let available: Vec<String> = available.into_iter().map(Into::into).collect();
    let fallback = fallback.into();
    if let Some(previous) = FOLLOW.take() {
        reactive_core::dispose_owner(previous);
    }
    let scope = reactive_core::detached(reactive_core::owner_scope);
    reactive_core::effect(move || {
        let preferred = preferences_core::use_preferred_locales();
        if preferred.is_empty() && i18n_core::current_locale().is_some() {
            return;
        }
        i18n_core::set_locale(i18n_core::negotiate_locale(
            &preferred, &available, &fallback,
        ));
    });
    FOLLOW.set(Some(scope.id()));
}

#[cfg(test)]
#[path = "system_locale_test.rs"]
mod tests;
