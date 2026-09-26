//! The user's system preferences as reactive state: written by the runner from `Event::SystemPreferencesChanged`, read by anything that should follow them.
//!
//! One signal per field, so a view that reads only the locales does not re-render when the colour scheme flips.
//!
//! Its own crate so the consumers below the facade can follow it without the facade: `theme-core` drives `follow_system` from the colour scheme, `motion-core` reads reduced motion on every frame, and `telar-plugin` writes a guest's copy across the plugin boundary. It sits on `platform-core` for the snapshot type and on `reactive-core` for the signals; `platform-core` stays clear of the reactive runtime, so the store cannot live there.

#![warn(rustdoc::broken_intra_doc_links)]

pub use platform_core::{ColorScheme, SystemPreferences};
use reactive_core::{RwSignal, detached, signal};

thread_local! {
    static COLOR_SCHEME: RwSignal<Option<ColorScheme>> = detached(|| signal(None));
    static REDUCED_MOTION: RwSignal<Option<bool>> = detached(|| signal(None));
    static HIGH_CONTRAST: RwSignal<Option<bool>> = detached(|| signal(None));
    static LOCALES: RwSignal<Vec<String>> = detached(|| signal(Vec::new()));
}

/// Replaces the preferences everything reads, notifying only the fields that changed. The runner calls this; a test or a tool without one can call it to stand in for the platform.
pub fn set_system_preferences(preferences: SystemPreferences) {
    let SystemPreferences {
        color_scheme,
        reduced_motion,
        high_contrast,
        locales,
    } = preferences;
    reactive_core::begin_batch();
    replace(&COLOR_SCHEME, color_scheme);
    replace(&REDUCED_MOTION, reduced_motion);
    replace(&HIGH_CONTRAST, high_contrast);
    replace(&LOCALES, locales);
    reactive_core::end_batch();
}

fn replace<T: Clone + PartialEq + 'static>(
    slot: &'static std::thread::LocalKey<RwSignal<T>>,
    value: T,
) {
    slot.with(|signal| {
        if signal.peek_with(|current| *current != value) {
            signal.set(value);
        }
    });
}

/// Reactive read of every field; subscribes the caller to all four.
pub fn use_system_preferences() -> SystemPreferences {
    SystemPreferences {
        color_scheme: use_color_scheme(),
        reduced_motion: use_reduced_motion(),
        high_contrast: use_high_contrast(),
        locales: use_preferred_locales(),
    }
}

/// Non-reactive read, for event handlers and bridges.
pub fn system_preferences() -> SystemPreferences {
    SystemPreferences {
        color_scheme: COLOR_SCHEME.with(|s| s.peek()),
        reduced_motion: REDUCED_MOTION.with(|s| s.peek()),
        high_contrast: HIGH_CONTRAST.with(|s| s.peek()),
        locales: LOCALES.with(|s| s.peek()),
    }
}

/// `None` until the platform reports one, and wherever it cannot.
pub fn use_color_scheme() -> Option<ColorScheme> {
    COLOR_SCHEME.with(|s| s.get())
}

/// `Some(true)` when the user asked for less motion; `None` where the platform cannot say.
pub fn use_reduced_motion() -> Option<bool> {
    REDUCED_MOTION.with(|s| s.get())
}

/// Non-reactive [`use_reduced_motion`], for the frame clock: it asks on every tick and must neither subscribe whatever happens to be running nor clone the locale list to answer.
pub fn reduced_motion() -> Option<bool> {
    REDUCED_MOTION.with(|s| s.peek())
}

/// `Some(true)` when the user asked for more contrast; `None` where the platform cannot say.
pub fn use_high_contrast() -> Option<bool> {
    HIGH_CONTRAST.with(|s| s.get())
}

/// BCP 47 tags, most preferred first; empty where the platform reports none.
pub fn use_preferred_locales() -> Vec<String> {
    LOCALES.with(|s| s.get())
}

#[cfg(test)]
#[path = "lib_test.rs"]
mod tests;
