//! Named theme modes on top of the low-level [`crate::set_theme`] store.
//!
//! An app registers each variant once (`register_mode`, or `register_mode_theme` with the theme itself) and switches by id (`set_mode`) instead of scattering `set_theme(...)` calls across the setup closure and every switch button. The active id lives in a reactive signal, so a label like `"Active · {mode}"` re-renders on switch without a hand-written memo, and the id is what the rsx crate bridges through hot-reload snapshot/restore so the selected variant survives a dylib swap.
//!
//! The id is the thread's unless the active surface carries one of its own ([`set_surface_mode`]), which is how a canvas shows a dark variant beside the light chrome around it: the mode-following token defaults read the surface's mode, and a mode registered with its theme lends that theme to the surface.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::mem::ManuallyDrop;
use std::rc::Rc;

use preferences_core::ColorScheme;
use reactive_core::{RwSignal, detached, signal};

use crate::ThemeTokens;
use crate::context::Installed;

// Installs a concrete theme (typically via `set_theme`). Type-erased so variants of any concrete theme type register under one string-keyed table.
type ApplyMode = Rc<dyn Fn()>;

reactive_core::surface_scoped! {
    /// The active surface's own mode, in place of the thread's while it has one.
    scoped mode: String as Option<String> = None;
    context ModeContext, ModeGuard;
}

thread_local! {
    // `ManuallyDrop` so no TLS destructor is registered and unmapping the app dylib on `dlclose` stays safe; the table is leaked at thread exit instead. A list rather than a map so the modes keep the order they were registered in.
    static MODES: ManuallyDrop<RefCell<Vec<(String, ApplyMode)>>> =
        const { ManuallyDrop::new(RefCell::new(Vec::new())) };
    // A signal so a surface showing a mode re-resolves its theme when that mode is registered anew.
    static MODE_THEMES: RwSignal<HashMap<String, Installed>> = detached(|| signal(HashMap::new()));
    // A signal so a surface already showing a mode re-resolves its mode-following tokens when the mode declares its scheme.
    static MODE_SCHEMES: RwSignal<HashMap<String, ColorScheme>> = detached(|| signal(HashMap::new()));
}

/// Registers a named mode. `apply` installs the concrete theme when the mode is selected. Re-registering an id replaces its closure, which is expected: hot reload re-runs the app's setup and re-registers every mode.
///
/// A mode registered this way has no theme a surface can borrow: a surface set to it resolves its mode-following tokens as that mode, over the application's theme. [`register_mode_theme`] registers one that has.
pub fn register_mode(id: impl Into<String>, apply: impl Fn() + 'static) {
    let id = id.into();
    forget_mode_theme(&id);
    insert_mode(id, Rc::new(apply));
}

/// Registers a named mode by the theme it stands for: selecting it with [`set_mode`] installs `theme` as [`set_theme`](crate::set_theme) would, and the theme is kept, so a surface set to this mode ([`set_surface_mode`]) and a [`ScopedTheme::for_mode`](crate::ScopedTheme::for_mode) can resolve it without installing it for the whole application. Re-registering an id replaces it.
pub fn register_mode_theme<T: ThemeTokens + Clone + 'static>(id: impl Into<String>, theme: T) {
    let id = id.into();
    let installed = Installed::new(theme);
    MODE_THEMES.with(|themes| {
        themes.update(|themes| {
            themes.insert(id.clone(), installed.clone());
        })
    });
    insert_mode(
        id,
        Rc::new(move || crate::context::install(installed.clone())),
    );
}

fn insert_mode(id: String, apply: ApplyMode) {
    MODES.with(|modes| {
        let mut modes = modes.borrow_mut();
        match modes.iter_mut().find(|(registered, _)| *registered == id) {
            Some((_, slot)) => *slot = apply,
            None => modes.push((id, apply)),
        }
    });
}

fn forget_mode_theme(id: &str) {
    if MODE_THEMES.with(|themes| themes.peek_with(|themes| themes.contains_key(id))) {
        MODE_THEMES.with(|themes| {
            themes.update(|themes| {
                themes.remove(id);
            })
        });
    }
}

/// Declares the colour scheme a mode is in, so [`is_dark`] and the mode-following token defaults treat it as light or dark whether or not it is part of a [`follow_system`] pair. Call it for a mode registered with [`register_mode`] or [`register_mode_theme`] that is not that pair, such as a high-contrast or a third variant. Survives re-registering the mode; a mode with no declared scheme falls back to the pair.
pub fn set_mode_scheme(id: impl Into<String>, scheme: ColorScheme) {
    let id = id.into();
    MODE_SCHEMES.with(|schemes| {
        if schemes.peek_with(|schemes| schemes.get(&id) != Some(&scheme)) {
            schemes.update(|schemes| {
                schemes.insert(id, scheme);
            });
        }
    });
}

/// Every registered mode id, in the order the modes were first registered. Not reactive: modes are registered while the application sets up.
pub fn registered_modes() -> Vec<String> {
    MODES.with(|modes| modes.borrow().iter().map(|(id, _)| id.clone()).collect())
}

/// The theme `id` was registered with through [`register_mode_theme`], read reactively.
fn mode_theme(id: &str) -> Option<Installed> {
    MODE_THEMES.with(|themes| themes.with(|themes| themes.get(id).cloned()))
}

/// [`mode_theme`] without subscribing the caller.
pub(crate) fn mode_theme_now(id: &str) -> Option<Installed> {
    MODE_THEMES.with(|themes| themes.peek_with(|themes| themes.get(id).cloned()))
}

/// The theme of the active surface's own mode, where it has one and that mode was registered with its theme. Reactive.
pub(crate) fn surface_mode_theme() -> Option<Installed> {
    mode().surface().and_then(|id| mode_theme(&id))
}

/// Selects a mode: runs its registered `apply` closure (if one is registered) and publishes the id to the reactive active-mode signal. Setting an unregistered id still updates the signal, so an app may drive the theme from its own effect on `use_mode` instead of registering closures.
pub fn set_mode(id: impl Into<String>) {
    let id = id.into();
    let apply = MODES.with(|modes| {
        modes
            .borrow()
            .iter()
            .find(|(registered, _)| *registered == id)
            .map(|(_, apply)| apply.clone())
    });
    if let Some(apply) = apply {
        apply();
    }
    mode().set(Some(id));
}

/// Gives the active surface a mode of its own, or with `None` hands it back to the thread's [`set_mode`]. Runs no `apply` closure: the application's theme stays as it is, and only what reads the mode on this surface follows.
pub fn set_surface_mode(id: Option<&str>) {
    mode().set_surface(id);
}

/// Reactive read of the active surface's own mode, `None` where it follows the thread's.
pub fn use_surface_mode() -> Option<String> {
    mode().surface()
}

/// Reactive read of the mode in force on the active surface — subscribes the caller so a label re-renders on switch. `None` before any mode is set.
pub fn use_mode() -> Option<String> {
    mode().get()
}

/// Non-reactive read of the thread's mode, whatever the active surface's own: the one [`set_mode`] selected, for the hot-reload snapshot bridge.
pub fn active_mode() -> Option<String> {
    mode().peek_thread()
}

thread_local! {
    // The (light, dark) mode-id pair, so is_dark can tell which registered mode is the dark one without the app hardcoding it. ManuallyDrop for the same dlclose-safety reason as MODES above. None until set_light_dark is called.
    static SCHEME_PAIR: ManuallyDrop<RefCell<Option<(String, String)>>> =
        const { ManuallyDrop::new(RefCell::new(None)) };
}

/// Designates which two registered modes form the light/dark pair, so [`is_dark`] can tell which one is currently active. Called by [`follow_system`]; both ids should also be registered via [`register_mode`]. Does not itself change the active mode.
fn set_light_dark(light: impl Into<String>, dark: impl Into<String>) {
    SCHEME_PAIR.with(|p| *p.borrow_mut() = Some((light.into(), dark.into())));
}

/// Reactive: `true` when the mode in force on the active surface is dark: the scheme it declared with [`set_mode_scheme`], else whether it is the designated dark mode of the [`follow_system`] pair. `false` when it is light, undeclared outside the pair, or no mode is active. Backs [`ThemeTokens`](crate::ThemeTokens)'s mode-following `ink`/`surface` defaults.
///
/// Under [`follow_system`] that is the resolved scheme, preference included (see [`use_resolved_scheme`]).
pub fn is_dark() -> bool {
    let Some(active) = use_mode() else {
        return false;
    };
    let declared =
        MODE_SCHEMES.with(|schemes| schemes.with(|schemes| schemes.get(&active).copied()));
    if let Some(scheme) = declared {
        return scheme == ColorScheme::Dark;
    }
    SCHEME_PAIR.with(|p| p.borrow().as_ref().is_some_and(|(_, dark)| active == *dark))
}

/// How the app chooses its colour scheme: after the system, or fixed by the person using it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SchemePreference {
    #[default]
    System,
    Light,
    Dark,
}

impl SchemePreference {
    /// The scheme this preference comes to while the system reports `system`. A system that reports none is taken as light.
    pub fn resolve(self, system: Option<ColorScheme>) -> ColorScheme {
        match self {
            SchemePreference::Light => ColorScheme::Light,
            SchemePreference::Dark => ColorScheme::Dark,
            SchemePreference::System => system.unwrap_or(ColorScheme::Light),
        }
    }

    /// The word a preference is stored and carried under.
    pub fn as_str(self) -> &'static str {
        match self {
            SchemePreference::System => "system",
            SchemePreference::Light => "light",
            SchemePreference::Dark => "dark",
        }
    }

    pub fn parse(word: &str) -> Option<Self> {
        Some(match word {
            "system" => SchemePreference::System,
            "light" => SchemePreference::Light,
            "dark" => SchemePreference::Dark,
            _ => return None,
        })
    }
}

thread_local! {
    static PREFERENCE: RwSignal<SchemePreference> = detached(|| signal(SchemePreference::default()));
}

/// Sets how the app chooses its colour scheme. A fixed choice holds until the app changes it again, whatever the system does meanwhile; `System` hands the choice back.
pub fn set_scheme_preference(preference: SchemePreference) {
    PREFERENCE.with(|p| {
        if p.peek() != preference {
            p.set(preference);
        }
    });
}

/// The preference now, without subscribing.
pub fn scheme_preference() -> SchemePreference {
    PREFERENCE.with(|p| p.peek())
}

/// The preference, read reactively.
pub fn use_scheme_preference() -> SchemePreference {
    PREFERENCE.with(|p| p.get())
}

/// The scheme the app is in: the preference resolved against the system's, read reactively.
pub fn use_resolved_scheme() -> ColorScheme {
    use_scheme_preference().resolve(preferences_core::use_color_scheme())
}

thread_local! {
    // An `Effect` handle is inert, so the effect gets a scope of its own that a re-call disposes; otherwise two followers would race to set the mode.
    static FOLLOW: Cell<Option<reactive_core::OwnerId>> = const { Cell::new(None) };
}

/// A handle to the resolved scheme, read with `.get()` like any other reactive source.
///
/// It is what `$scheme` names in `.rsx`: `$scheme` is `scheme.get()`, which is [`use_resolved_scheme`] read inside whatever closure asks, so it follows a change of the system's scheme or of the [`SchemePreference`]. Zero-sized and `Copy`, so it moves into any number of closures with nothing to clone.
#[derive(Clone, Copy, Debug, Default)]
pub struct ResolvedScheme;

impl ResolvedScheme {
    pub fn get(&self) -> ColorScheme {
        use_resolved_scheme()
    }
}

/// Drives the active mode from the resolved scheme ([`use_resolved_scheme`]) — light → `light`, dark → `dark` — updating live as the system's scheme or the [`SchemePreference`] changes. Also designates the pair so `is_dark` stays consistent. Re-calling replaces the previous follower.
///
/// Under `System`, an unknown scheme is not a vote for light: it leaves an active mode alone, and selects `light` only when no mode is active yet. A manual [`set_mode`] of another mode still wins until the next change re-drives it; a person choosing light or dark is [`set_scheme_preference`], which holds.
pub fn follow_system(light: impl Into<String>, dark: impl Into<String>) {
    let light = light.into();
    let dark = dark.into();
    set_light_dark(light.clone(), dark.clone());
    if let Some(previous) = FOLLOW.take() {
        reactive_core::dispose_owner(previous);
    }
    let scope = detached(reactive_core::owner_scope);
    reactive_core::effect(move || {
        let preference = use_scheme_preference();
        let system = preferences_core::use_color_scheme();
        if preference == SchemePreference::System && system.is_none() && active_mode().is_some() {
            return;
        }
        let want = match preference.resolve(system) {
            ColorScheme::Dark => &dark,
            ColorScheme::Light => &light,
        };
        set_mode(want.clone());
    });
    FOLLOW.set(Some(scope.id()));
}

#[cfg(test)]
#[path = "mode_test.rs"]
mod tests;
