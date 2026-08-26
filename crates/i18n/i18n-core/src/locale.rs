//! The active-locale signal — the reactive source that drives live language switching.
//!
//! Mirrors `theme-core`'s active-mode store exactly: a thread-local `RwSignal` per surface, a setter, and a
//! reactive getter. Because every translated string compiles to a `Fn() -> String` closure that calls
//! [`use_locale`] (through [`crate::translate`]), switching the locale re-runs only the closures that read it
//! — the same fine-grained mechanism that re-paints only the widgets reading a theme token.

use std::mem::ManuallyDrop;

use reactive_core::{RwSignal, detached, signal};

thread_local! {
    // ManuallyDrop mirrors theme-core's signals: no TLS destructor is registered, so unmapping the dylib on
    // dlclose stays safe. Cleanup happens via reset_runtime() dropping the whole Runtime.
    static LOCALE: ManuallyDrop<RwSignal<Option<String>>> = ManuallyDrop::new(detached(|| signal(None)));
}

/// Sets the active locale (a BCP-47 tag such as `"en"` or `"es"`), re-rendering every translated string that
/// reads it. The tag should be one of the baked catalog's locales; an unknown tag simply falls back to the
/// catalog's default locale at lookup time.
///
/// **Scope: one reactive runtime, which is one UI thread** — the same as the theme, the control size and the
/// text direction, all of which are thread-local signals for the same reason (a signal is `!Send`). Telar's
/// own multi-surface runner drives every surface from one runtime, so there this reaches all of them. An
/// out-of-tree platform that runs a runtime *per* thread owns fanning process-wide state across its threads,
/// and owns it for all four signals rather than for this one: a locale-shaped broadcast in here would leave
/// the same backend still hand-rolling the other three.
pub fn set_locale(id: impl Into<String>) {
    LOCALE.with(|s| s.set(Some(id.into())));
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
/// or the C/POSIX locale. An app can seed the initial language by passing this to [`set_locale`] at startup.
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
