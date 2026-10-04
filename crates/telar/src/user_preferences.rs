//! What a person chose, kept between runs in the store of the target the app runs on, and the choices Telar keeps for itself.

use theme_core::SchemePreference;

/// The key the colour scheme a person chose is kept under.
pub const SCHEME_KEY: &str = "telar.scheme";

/// The key the locale a person last read the app in is kept under, by an app whose address carries it (see [`follow_location_locale`](crate::follow_location_locale)).
pub const LOCALE_KEY: &str = "telar.locale";

/// The key the app's reduced-motion override is kept under (see [`set_reduced_motion_override`](crate::set_reduced_motion_override)): `true` or `false`, and absent while the app follows the system.
pub const REDUCED_MOTION_KEY: &str = "telar.reduced_motion";

/// Installs the target's preference store for `app_name` and brings back what Telar keeps in it. Every runner calls it once, where it installs the app's paths.
pub(crate) fn install(app_name: &str) {
    install_store(app_name);
    follow_stored_choices();
}

/// Installs the target's preference store for `app_name` alone, for a runner that brings back what is kept in it later: one taking over a prerendered page builds its first tree from the choices the page was written with.
pub(crate) fn install_store(app_name: &str) {
    #[cfg(all(feature = "web-dom", target_arch = "wasm32"))]
    services_core::set_preference_store(std::sync::Arc::new(platform_web::WebStorage::new(
        app_name,
    )));
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(dir) = services_core::app_paths::config() {
        services_core::set_preference_store(std::sync::Arc::new(services_core::FileStore::open(
            dir.join("preferences"),
        )));
    }
    #[cfg(not(all(feature = "web-dom", target_arch = "wasm32")))]
    let _ = app_name;
}

/// Restores every choice Telar keeps in the installed store, then keeps each as it changes.
pub(crate) fn follow_stored_choices() {
    follow_stored_scheme();
    follow_stored_reduced_motion();
}

/// Restores the scheme a person chose, then keeps it as they change it.
pub(crate) fn follow_stored_scheme() {
    if let Some(preference) =
        services_core::stored_preference(SCHEME_KEY).and_then(|word| SchemePreference::parse(&word))
    {
        theme_core::set_scheme_preference(preference);
    }
    let _scope = reactive_core::detached(reactive_core::owner_scope);
    reactive_core::effect(|| {
        let preference = theme_core::use_scheme_preference();
        services_core::store_preference(SCHEME_KEY, Some(preference.as_str()));
    });
}

/// Restores the reduced-motion override a person chose, then keeps it as they change it.
pub(crate) fn follow_stored_reduced_motion() {
    if let Some(reduced) =
        services_core::stored_preference(REDUCED_MOTION_KEY).and_then(|word| word.parse().ok())
    {
        preferences_core::set_reduced_motion_override(Some(reduced));
    }
    let _scope = reactive_core::detached(reactive_core::owner_scope);
    reactive_core::effect(|| {
        let word =
            preferences_core::use_reduced_motion_override().map(|reduced| reduced.to_string());
        services_core::store_preference(REDUCED_MOTION_KEY, word.as_deref());
    });
}

#[cfg(test)]
#[path = "user_preferences_test.rs"]
mod tests;
