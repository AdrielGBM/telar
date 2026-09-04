//! Named theme modes on top of the low-level [`crate::set_theme`] store.
//!
//! An app registers each variant once (`register_mode`) and switches by id (`set_mode`) instead of scattering `set_theme(...)` calls across the setup closure and every switch button. The active id lives in a reactive signal, so a label like `"Active · {mode}"` re-renders on switch without a hand-written memo, and the id is what the rsx crate bridges through hot-reload snapshot/restore so the selected variant survives a dylib swap.

use std::cell::RefCell;
use std::collections::HashMap;
use std::mem::ManuallyDrop;
use std::rc::Rc;

use reactive_core::{RwSignal, detached, signal};

// Installs a concrete theme (typically via `set_theme`). Type-erased so variants of any concrete theme type register under one string-keyed table.
type ApplyMode = Rc<dyn Fn()>;

thread_local! {
    // ManuallyDrop mirrors THEME/WIDGET_THEME in context.rs: no TLS destructor is registered, so unmapping the dylib on dlclose stays safe. Cleanup happens via reset_runtime() dropping the whole Runtime.
    static ACTIVE_MODE: RwSignal<Option<String>> = detached(|| signal(None));
    static MODES: ManuallyDrop<RefCell<HashMap<String, ApplyMode>>> =
        ManuallyDrop::new(RefCell::new(HashMap::new()));
}

/// Registers a named mode. `apply` installs the concrete theme when the mode is selected. Re-registering an id replaces its closure, which is expected: hot reload re-runs the app's setup and re-registers every mode.
pub fn register_mode(id: impl Into<String>, apply: impl Fn() + 'static) {
    MODES.with(|m| m.borrow_mut().insert(id.into(), Rc::new(apply)));
}

/// Selects a mode: runs its registered `apply` closure (if one is registered) and publishes the id to the reactive active-mode signal. Setting an unregistered id still updates the signal, so an app may drive the theme from its own effect on `use_mode` instead of registering closures.
pub fn set_mode(id: impl Into<String>) {
    let id = id.into();
    let apply = MODES.with(|m| m.borrow().get(&id).cloned());
    if let Some(apply) = apply {
        apply();
    }
    ACTIVE_MODE.with(|s| s.set(Some(id)));
}

/// Reactive read of the active mode id — subscribes the caller so a label re-renders on switch. `None` before any mode is set.
fn use_mode() -> Option<String> {
    ACTIVE_MODE.with(|s| s.get())
}

/// Non-reactive read of the active mode id, for the hot-reload snapshot bridge.
pub fn active_mode() -> Option<String> {
    ACTIVE_MODE.with(|s| s.peek())
}

thread_local! {
    // The (light, dark) mode-id pair, so is_dark can tell which registered mode is the dark one without the app hardcoding it. ManuallyDrop for the same dlclose-safety reason as MODES/ACTIVE_MODE above. None until set_light_dark is called.
    static SCHEME_PAIR: ManuallyDrop<RefCell<Option<(String, String)>>> =
        ManuallyDrop::new(RefCell::new(None));
}

/// Designates which two registered modes form the light/dark pair, so [`is_dark`] can tell which one is currently active. Called by [`follow_system`]; both ids should also be registered via [`register_mode`]. Does not itself change the active mode.
fn set_light_dark(light: impl Into<String>, dark: impl Into<String>) {
    SCHEME_PAIR.with(|p| *p.borrow_mut() = Some((light.into(), dark.into())));
}

/// Reactive: `true` when the active mode is the designated dark mode. `false` when it is the light mode, no pair has been set, or a third (unpaired) mode is active. Backs [`ThemeTokens`](crate::ThemeTokens)'s mode-following `ink`/`surface` defaults.
pub(crate) fn is_dark() -> bool {
    let active = use_mode();
    SCHEME_PAIR.with(|p| {
        p.borrow()
            .as_ref()
            .is_some_and(|(_, dark)| active.as_deref() == Some(dark.as_str()))
    })
}

thread_local! {
    // OS light/dark preference, fed by set_system_dark from the platform layer and read reactively by the follow_system effect. ManuallyDrop for the same dlclose-safety reason as the signals above.
    static SYSTEM_DARK: RwSignal<bool> = detached(|| signal(false));
    // Keeps the follow_system effect alive for the app's lifetime; replaced (old dropped) on re-call, since a hot reload re-runs the app's setup.
    static FOLLOW: ManuallyDrop<RefCell<Option<reactive_core::Effect>>> =
        ManuallyDrop::new(RefCell::new(None));
}

/// Reports the OS light/dark preference into the reactive graph. Called by the runner at window creation and whenever the OS scheme changes; drives [`follow_system`].
pub fn set_system_dark(dark: bool) {
    SYSTEM_DARK.with(|s| s.set(dark));
}

/// Drives the active mode from the OS light/dark preference — light → `light`, dark → `dark` — updating live as the OS scheme changes. Installs a reactive effect (kept alive internally) and designates the pair so `is_dark` stays consistent. Re-calling replaces the effect (hot reload re-runs setup). A manual [`set_mode`] still wins until the next OS change re-drives it.
pub fn follow_system(light: impl Into<String>, dark: impl Into<String>) {
    let light = light.into();
    let dark = dark.into();
    set_light_dark(light.clone(), dark.clone());
    let eff = reactive_core::effect(move || {
        let want = if SYSTEM_DARK.with(|s| s.get()) {
            &dark
        } else {
            &light
        };
        set_mode(want.clone());
    });
    FOLLOW.with(|f| *f.borrow_mut() = Some(eff));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn reset() {
        ACTIVE_MODE.with(|s| s.set(None));
        MODES.with(|m| m.borrow_mut().clear());
        SCHEME_PAIR.with(|p| *p.borrow_mut() = None);
        // Drop any prior follow_system effect first, so it stops reacting to SYSTEM_DARK in later tests.
        FOLLOW.with(|f| *f.borrow_mut() = None);
        SYSTEM_DARK.with(|s| s.set(false));
    }

    #[test]
    fn set_mode_runs_apply_and_publishes_id() {
        reset();
        let hits = Rc::new(Cell::new(0));
        let h = hits.clone();
        register_mode("dark", move || h.set(h.get() + 1));
        set_mode("dark");
        assert_eq!(hits.get(), 1, "apply closure ran once");
        assert_eq!(active_mode().as_deref(), Some("dark"));
    }

    #[test]
    fn set_mode_publishes_even_without_registration() {
        reset();
        set_mode("unregistered");
        assert_eq!(active_mode().as_deref(), Some("unregistered"));
    }

    #[test]
    fn follow_system_drives_mode_from_os_scheme() {
        reset();
        register_mode("day", || {});
        register_mode("night", || {});
        follow_system("day", "night");
        assert_eq!(
            active_mode().as_deref(),
            Some("day"),
            "effect runs once with default SYSTEM_DARK=false → light"
        );
        set_system_dark(true);
        assert_eq!(active_mode().as_deref(), Some("night"));
        set_system_dark(false);
        assert_eq!(active_mode().as_deref(), Some("day"));
    }

    #[test]
    fn is_dark_false_for_unpaired_third_mode() {
        reset();
        set_light_dark("day", "night");
        set_mode("pastel");
        assert!(
            !is_dark(),
            "a third mode outside the pair is neither dark nor light"
        );
    }

    #[test]
    fn use_mode_is_reactive() {
        reset();
        let seen = Rc::new(RefCell::new(Vec::<Option<String>>::new()));
        let s = seen.clone();
        let _e = reactive_core::effect(move || s.borrow_mut().push(use_mode()));
        set_mode("a");
        set_mode("b");
        let got = seen.borrow().clone();
        assert_eq!(
            got,
            vec![None, Some("a".into()), Some("b".into())],
            "effect re-ran on each mode switch"
        );
    }
}
