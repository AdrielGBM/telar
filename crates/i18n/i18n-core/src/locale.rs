//! The active-locale signal — the reactive source that drives live language switching.
//!
//! Mirrors `theme-core`'s active-mode store exactly: a thread-local `RwSignal` per surface, a setter, and a
//! reactive getter. Because every translated string compiles to a `Fn() -> String` closure that calls
//! [`use_locale`] (through [`crate::translate`]), switching the locale re-runs only the closures that read it
//! — the same fine-grained mechanism that re-paints only the widgets reading a theme token.

use std::mem::ManuallyDrop;

use reactive_core::{RwSignal, signal};

thread_local! {
    // ManuallyDrop mirrors theme-core's signals: no TLS destructor is registered, so unmapping the dylib on
    // dlclose stays safe. Cleanup happens via reset_runtime() dropping the whole Runtime.
    static LOCALE: ManuallyDrop<RwSignal<Option<String>>> = ManuallyDrop::new(signal(None));
}

/// Sets the active locale (a BCP-47 tag such as `"en"` or `"es"`), re-rendering every translated string that
/// reads it. The tag should be one of the baked catalog's locales; an unknown tag simply falls back to the
/// catalog's default locale at lookup time.
pub fn set_locale(id: impl Into<String>) {
    LOCALE.with(|s| s.set(Some(id.into())));
}

/// Sets `default` only when no locale is active yet. Called at app start (and after a hot reload) so a
/// selection restored across a dylib swap is not clobbered by the default.
pub fn init_locale(default: impl Into<String>) {
    if LOCALE.with(|s| s.peek().is_none()) {
        set_locale(default);
    }
}

/// Reactive read of the active locale — subscribes the caller so translated text re-renders on switch. `None`
/// before any locale is set (callers fall back to the catalog's default locale).
pub fn use_locale() -> Option<String> {
    LOCALE.with(|s| s.get())
}

/// Non-reactive read of the active locale, for the hot-reload snapshot bridge and event handlers.
pub fn current_locale() -> Option<String> {
    LOCALE.with(|s| s.peek())
}

/// The language subtag of the OS locale, from `$LC_ALL` / `$LC_MESSAGES` / `$LANG` (in POSIX precedence),
/// lowercased and stripped of any territory/encoding suffix — e.g. `es_ES.UTF-8` → `"es"`. `None` when unset
/// or the C/POSIX locale. An app can seed the initial language with `init_locale(detect_system_locale()?)`.
pub fn detect_system_locale() -> Option<String> {
    let raw = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .into_iter()
        .find_map(|var| std::env::var(var).ok())
        .filter(|v| !v.is_empty())?;
    let lang = raw
        .split(['.', '@'])
        .next()
        .unwrap_or(&raw)
        .split('_')
        .next()
        .unwrap_or(&raw)
        .to_ascii_lowercase();
    if lang.is_empty() || lang == "c" || lang == "posix" {
        return None;
    }
    Some(lang)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reset() {
        LOCALE.with(|s| s.set(None));
    }

    #[test]
    fn init_locale_does_not_clobber_existing_selection() {
        reset();
        set_locale("es");
        init_locale("en");
        assert_eq!(current_locale().as_deref(), Some("es"));
    }

    #[test]
    fn init_locale_applies_default_when_empty() {
        reset();
        init_locale("en");
        assert_eq!(current_locale().as_deref(), Some("en"));
    }

    #[test]
    fn use_locale_is_reactive() {
        reset();
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::<Option<String>>::new()));
        let s = seen.clone();
        let _e = reactive_core::effect(move || s.borrow_mut().push(use_locale()));
        set_locale("en");
        set_locale("es");
        assert_eq!(
            *seen.borrow(),
            vec![None, Some("en".into()), Some("es".into())],
            "effect re-ran on each locale switch"
        );
    }
}
