//! The active locale: a thread-wide signal, and a per-surface override that takes its place on the surface that sets one.
//!
//! Every translated string compiles to a `Fn() -> String` closure that calls [`use_locale`] (through [`crate::translate`]), so switching either re-runs only the closures that read it — the same fine-grained mechanism that re-paints only the widgets reading a theme token.

reactive_core::surface_scoped! {
    /// The active surface's own locale, in place of the thread's while it has one.
    scoped locale: String as Option<String> = None;
    context LocaleContext, LocaleGuard;
}

/// Sets the active locale (a BCP-47 tag such as `"en"` or `"es"`), re-rendering every translated string that reads it on a surface without a locale of its own. The tag should be one of the baked catalog's locales; an unknown tag simply falls back to the catalog's default locale at lookup time.
///
/// **Scope: one reactive runtime, which is one UI thread** — the same as the theme, the control size and the text direction, all of which are thread-local signals for the same reason (a signal is `!Send`). Telar's own multi-surface runner drives every surface from one runtime, so there this reaches all of them. An out-of-tree platform that runs a runtime *per* thread owns fanning process-wide state across its threads, and owns it for all four signals rather than for this one: a locale-shaped broadcast in here would leave the same backend still hand-rolling the other three.
pub fn set_locale(id: impl Into<String>) {
    locale().set(Some(id.into()));
}

/// Gives the active surface a locale of its own, or with `None` hands it back to the thread's [`set_locale`].
pub fn set_surface_locale(id: Option<&str>) {
    locale().set_surface(id);
}

/// Reactive read of the active surface's own locale, `None` where it follows the thread's.
pub fn use_surface_locale() -> Option<String> {
    locale().surface()
}

/// Reactive read of the locale in force on the active surface — subscribes the caller so translated text re-renders on switch. `None` before any locale is set (callers fall back to the catalog's default locale).
pub fn use_locale() -> Option<String> {
    locale().get()
}

/// Non-reactive read of the locale in force on the active surface, for the hot-reload snapshot bridge and event handlers.
pub fn current_locale() -> Option<String> {
    locale().peek()
}

#[cfg(test)]
#[path = "locale_test.rs"]
mod tests;
