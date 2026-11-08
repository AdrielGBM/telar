//! What a person chose, kept between runs in the store of the target the app runs on, and the choices Telar keeps for itself.

use std::cell::Cell;
use std::sync::Once;

use telar_project::protocol::RendererBackend;
use theme_core::SchemePreference;

/// The key the colour scheme a person chose is kept under.
pub const SCHEME_KEY: &str = "telar.scheme";

/// The key the locale a person last read the app in is kept under, by an app whose address carries it (see [`follow_location_locale`](crate::follow_location_locale)).
pub const LOCALE_KEY: &str = "telar.locale";

/// The key the app's reduced-motion override is kept under (see [`set_reduced_motion_override`](crate::set_reduced_motion_override)): `true` or `false`, and absent while the app follows the system.
pub const REDUCED_MOTION_KEY: &str = "telar.reduced_motion";

/// The key the renderer backend a person switched to is kept under, as `RendererBackend::as_str` writes it.
pub const BACKEND_KEY: &str = "telar.backend";

/// The key a command-line or terminal app's history is kept under when it last moved: one entry per line, root-first, each as `LocationFormat::root` writes it, which never leaves a raw line break in one.
pub const HISTORY_KEY: &str = "telar.history";

reactive_core::dropless_thread_local! {
    static FOLLOWING: Cell<bool> = const { Cell::new(false) };
}

/// Installs the target's preference store for `app_name`, once per process: a store opened again for a later surface would hold only what its file held when read, and write that back over whatever another thread kept since.
pub(crate) fn install_store(app_name: &str) {
    static INSTALLED: Once = Once::new();
    INSTALLED.call_once(|| {
        #[cfg(all(feature = "web-dom", target_arch = "wasm32"))]
        services_core::set_preference_store(std::sync::Arc::new(platform_web::WebStorage::new(
            app_name,
        )));
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(dir) = services_core::app_paths::config() {
            services_core::set_preference_store(std::sync::Arc::new(
                services_core::FileStore::open(dir.join("preferences")),
            ));
        }
        #[cfg(not(all(feature = "web-dom", target_arch = "wasm32")))]
        let _ = app_name;
    });
}

pub(crate) fn stored_backend() -> Option<RendererBackend> {
    services_core::stored_preference(BACKEND_KEY)
        .and_then(|word| RendererBackend::parse(&word).ok())
}

pub(crate) fn store_backend(backend: RendererBackend) {
    services_core::store_preference(BACKEND_KEY, Some(backend.as_str()));
}

pub(crate) fn stored_history() -> Vec<String> {
    services_core::stored_preference(HISTORY_KEY)
        .map(|text| text.lines().map(str::to_owned).collect())
        .unwrap_or_default()
}

pub(crate) fn store_history(history: &[String]) {
    let text = history.join("\n");
    services_core::store_preference(HISTORY_KEY, (!history.is_empty()).then_some(text.as_str()));
}

/// Restores every choice Telar keeps in the installed store, then keeps each as it changes, once per thread: the choices live in the thread's reactive runtime, and a second follower there would write every change again.
pub(crate) fn follow_stored_choices() {
    if FOLLOWING.replace(true) {
        return;
    }
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
#[path = "user_preferences.test.rs"]
mod tests;
