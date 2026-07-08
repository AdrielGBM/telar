//! Named theme modes on top of the low-level [`crate::set_theme`] store.
//!
//! An app registers each variant once (`register_mode`) and switches by id (`set_mode`) instead of
//! scattering `set_theme(...)` calls across the setup closure and every switch button. The
//! active id lives in a reactive signal, so a label like `"Active · {mode}"` re-renders on switch without a
//! hand-written memo, and the id is what the rsx crate bridges through hot-reload snapshot/restore so the
//! selected variant survives a dylib swap.

use std::cell::RefCell;
use std::collections::HashMap;
use std::mem::ManuallyDrop;
use std::rc::Rc;

use reactive_core::{RwSignal, signal};

// Installs a concrete theme (typically via `set_theme`). Type-erased so variants of any concrete
// theme type register under one string-keyed table.
type ApplyMode = Rc<dyn Fn()>;

thread_local! {
    // ManuallyDrop mirrors THEME/WIDGET_THEME in context.rs: no TLS destructor is registered, so unmapping the
    // dylib on dlclose stays safe. Cleanup happens via reset_runtime() dropping the whole Runtime.
    static ACTIVE_MODE: ManuallyDrop<RwSignal<Option<String>>> = ManuallyDrop::new(signal(None));
    static MODES: ManuallyDrop<RefCell<HashMap<String, ApplyMode>>> =
        ManuallyDrop::new(RefCell::new(HashMap::new()));
}

/// Registers a named mode. `apply` installs the concrete theme when the mode is selected. Re-registering an
/// id replaces its closure, which is expected: hot reload re-runs the app's setup and re-registers every mode.
pub fn register_mode(id: impl Into<String>, apply: impl Fn() + 'static) {
    MODES.with(|m| m.borrow_mut().insert(id.into(), Rc::new(apply)));
}

/// Selects a mode: runs its registered `apply` closure (if one is registered) and publishes the id to the
/// reactive active-mode signal. Setting an unregistered id still updates the signal, so an app may drive the
/// theme from its own effect on [`use_mode`] instead of registering closures.
pub fn set_mode(id: impl Into<String>) {
    let id = id.into();
    let apply = MODES.with(|m| m.borrow().get(&id).cloned());
    if let Some(apply) = apply {
        apply();
    }
    ACTIVE_MODE.with(|s| s.set(Some(id)));
}

/// Selects `default` only when no mode is active yet. Called at app start and after a hot reload so a
/// selection restored by the rsx hot-reload bridge is not clobbered by the default.
pub fn init_mode(default: impl Into<String>) {
    let already_set = ACTIVE_MODE.with(|s| s.peek().is_some());
    if !already_set {
        set_mode(default);
    }
}

/// Reactive read of the active mode id — subscribes the caller so a label re-renders on switch. `None` before
/// any mode is set.
pub fn use_mode() -> Option<String> {
    ACTIVE_MODE.with(|s| s.get())
}

/// Non-reactive read of the active mode id, for the hot-reload snapshot bridge.
pub fn active_mode() -> Option<String> {
    ACTIVE_MODE.with(|s| s.peek())
}

thread_local! {
    // The (light, dark) mode-id pair, so is_dark/set_dark/toggle_dark can switch schemes without the app
    // hardcoding which registered modes are the light/dark ones. ManuallyDrop for the same dlclose-safety
    // reason as MODES/ACTIVE_MODE above. None until set_light_dark is called.
    static SCHEME_PAIR: ManuallyDrop<RefCell<Option<(String, String)>>> =
        ManuallyDrop::new(RefCell::new(None));
}

/// Designates which two registered modes form the light/dark pair. A thin, optional convention over the open
/// mode registry: it does not replace named modes (a third mode like `"pastel"` stays valid) — it only tells
/// `is_dark`/`set_dark`/`toggle_dark` which ids to flip between. Both ids should also be registered via
/// [`register_mode`]. Does not itself change the active mode.
pub fn set_light_dark(light: impl Into<String>, dark: impl Into<String>) {
    SCHEME_PAIR.with(|p| *p.borrow_mut() = Some((light.into(), dark.into())));
}

/// Reactive: `true` when the active mode is the designated dark mode. `false` when it is the light mode, no
/// pair has been set, or a third (unpaired) mode is active. Read this for a sun/moon toggle's on/off state.
pub fn is_dark() -> bool {
    let active = use_mode();
    SCHEME_PAIR.with(|p| {
        p.borrow()
            .as_ref()
            .is_some_and(|(_, dark)| active.as_deref() == Some(dark.as_str()))
    })
}

/// Selects the designated dark (`on = true`) or light (`on = false`) mode. No-op if no pair has been set.
pub fn set_dark(on: bool) {
    let target = SCHEME_PAIR.with(|p| {
        p.borrow()
            .as_ref()
            .map(|(light, dark)| if on { dark.clone() } else { light.clone() })
    });
    if let Some(target) = target {
        set_mode(target);
    }
}

/// Flips between the designated light and dark modes. Reads the current scheme non-reactively so it is safe
/// to call from an event handler.
pub fn toggle_dark() {
    let currently_dark = SCHEME_PAIR.with(|p| {
        p.borrow()
            .as_ref()
            .is_some_and(|(_, dark)| active_mode().as_deref() == Some(dark.as_str()))
    });
    set_dark(!currently_dark);
}

thread_local! {
    // OS light/dark preference, fed by set_system_dark from the platform layer and read reactively by the
    // follow_system effect. ManuallyDrop for the same dlclose-safety reason as the signals above.
    static SYSTEM_DARK: ManuallyDrop<RwSignal<bool>> = ManuallyDrop::new(signal(false));
    // Keeps the follow_system effect alive for the app's lifetime; replaced (old dropped) on re-call, since a
    // hot reload re-runs the app's setup.
    static FOLLOW: ManuallyDrop<RefCell<Option<reactive_core::Effect>>> =
        ManuallyDrop::new(RefCell::new(None));
}

/// Reports the OS light/dark preference into the reactive graph. Called by the runner at window creation and
/// whenever the OS scheme changes; drives [`follow_system`].
pub fn set_system_dark(dark: bool) {
    SYSTEM_DARK.with(|s| s.set(dark));
}

/// Drives the active mode from the OS light/dark preference — light → `light`, dark → `dark` — updating live
/// as the OS scheme changes. Installs a reactive effect (kept alive internally) and designates the pair so
/// [`is_dark`]/[`toggle_dark`] stay consistent. Re-calling replaces the effect (hot reload re-runs setup). A
/// manual [`set_mode`] still wins until the next OS change re-drives it.
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

    // Thread-locals persist across tests sharing a runner thread; each test resets to a known-empty state.
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
    fn init_mode_does_not_clobber_existing_selection() {
        reset();
        set_mode("midnight");
        init_mode("modern");
        assert_eq!(
            active_mode().as_deref(),
            Some("midnight"),
            "init must keep a selection already made (e.g. restored across hot reload)"
        );
    }

    #[test]
    fn init_mode_applies_default_when_empty() {
        reset();
        init_mode("modern");
        assert_eq!(active_mode().as_deref(), Some("modern"));
    }

    #[test]
    fn set_and_toggle_dark_switch_between_the_pair() {
        reset();
        register_mode("day", || {});
        register_mode("night", || {});
        set_light_dark("day", "night");

        set_dark(true);
        assert_eq!(active_mode().as_deref(), Some("night"));
        set_dark(false);
        assert_eq!(active_mode().as_deref(), Some("day"));

        toggle_dark();
        assert_eq!(active_mode().as_deref(), Some("night"));
        toggle_dark();
        assert_eq!(active_mode().as_deref(), Some("day"));
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
    fn dark_helpers_are_noops_without_a_pair() {
        reset();
        set_dark(true);
        toggle_dark();
        assert_eq!(active_mode(), None, "no pair set → nothing to switch to");
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
