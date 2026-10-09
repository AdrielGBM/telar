//! The installed theme: setting one, and reading it back as the app's own type, the token trait, or a plugin's own tokens.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::marker::PhantomData;
use std::rc::Rc;

use geometry_core::Color;
use reactive_core::{RwSignal, detached, signal};
use renderer_core::Declared;

/// Opt-in semantic-token contract the built-in component catalogue reads through, so a component can resolve a token without knowing the concrete theme type.
///
/// **Every method carries a default**, which makes `impl ThemeTokens for MyTheme {}` valid and each token an independent opt-in: a theme answers the questions it cares about and lets the catalogue keep its own answer for the rest. This trait is deliberately not where a theme's vocabulary lives — that belongs to the theme's own type, reachable in full through [`use_theme`]. What is here is only the subset a component written without knowledge of that type has to be able to ask for.
///
/// The metric tokens are *bases*, not a size scale: a component derives its own proportions from one rather than asking for a named role, because naming the roles would decide for every application which roles may exist. One number scales the thing; the component keeps its own ratios.
///
/// Nothing here answers what size the text is, and that is the point. Text size inherits, so it is not a question a component asks a theme — it is one it asks the region it is standing in. A theme that wants to move it declares it once, at [`root`](Self::root).
pub trait ThemeTokens: 'static {
    /// The accent a filled control, a link and the focus ring are drawn in. Mode-following, because one blue cannot carry white text at 4.5:1 and also read as text on a dark surface.
    fn primary(&self) -> Color {
        if crate::mode::is_dark() {
            Color::rgba(0.42, 0.62, 1.0, 1.0)
        } else {
            Color::rgba(0.15, 0.39, 0.92, 1.0)
        }
    }
    /// Readable ink on [`primary`](Self::primary), at 4.5:1 or more in either mode.
    fn on_primary(&self) -> Color {
        if crate::mode::is_dark() {
            Color::rgba(0.04, 0.06, 0.14, 1.0)
        } else {
            Color::rgba(1.0, 1.0, 1.0, 1.0)
        }
    }

    /// Base corner radius in px. A component rounds by this, or by a step of the scale below where its shape asks for one (a pill is not a card).
    fn radius(&self) -> f32 {
        4.0
    }

    /// The steps either side of [`radius`](Self::radius), so a theme owns **how round everything is** instead of each component keeping its own literal.
    ///
    /// This is the axis an application actually restyles, and a scale of three steps derived from one base is what a design system needs to be reachable from outside. A component that hardcodes `BorderRadius::all(8.0)` is not themeable at all — the caller can change the base radius and watch nothing move — and the fix is not a prop per component but a token they all read.
    ///
    /// A theme that wants a flat scale returns the same number from all three; one that wants a rounder language moves the base and the steps follow.
    fn radius_sm(&self) -> f32 {
        self.radius() * 0.6
    }
    fn radius_md(&self) -> f32 {
        self.radius() * 0.8
    }
    fn radius_lg(&self) -> f32 {
        self.radius()
    }
    /// Base gap between adjacent things in px, and the unit a component derives its own padding from.
    fn spacing(&self) -> f32 {
        8.0
    }
    /// What this theme puts at the root of the document, over [`ink`](Self::ink) and the document's own size.
    ///
    /// Every property of a [`Declared`] is an *inheriting* one, which makes this the only honest place for a theme to set one: said here it is a property of the document that anything below can override, rather than an answer each component has to remember to ask for. Said as a token it would be a second channel for a value the cascade already carries, and the two disagree the moment a region declares its own — which is how a hint ends up in the theme's near-black inside a panel written in white.
    ///
    /// This is also the whole of a theme's typography. Eight of the ten inherited properties never had a token at all, so a theme with a face or a leading of its own had to write it at every call site.
    fn root(&self) -> Declared {
        Declared::default()
    }
    /// Default size of a standalone icon in px.
    fn icon_size(&self) -> f32 {
        16.0
    }

    /// The steps either side of [`spacing`](Self::spacing), so a theme owns **how much air everything has** instead of each component keeping its own literal.
    ///
    /// Note the base sits in the *middle* here, where the radius base is the largest step: "how round is the biggest thing" and "what is the default gap" are different questions, and a scale that pretended otherwise would make every component either cramped or airy the moment a theme moved one number.
    ///
    /// A theme that wants a flat rhythm returns the same number from all four; one that wants a roomier language moves the base and the steps follow.
    fn spacing_sm(&self) -> f32 {
        self.spacing() * 0.5
    }
    fn spacing_md(&self) -> f32 {
        self.spacing()
    }
    fn spacing_lg(&self) -> f32 {
        self.spacing() * 1.5
    }
    fn spacing_xl(&self) -> f32 {
        self.spacing() * 2.0
    }

    /// Secondary text: captions and hints. Opaque and mode-following so it still reads at 4.5:1 on [`surface`](Self::surface) and on [`surface_alt`](Self::surface_alt) over it.
    fn muted(&self) -> Color {
        if crate::mode::is_dark() {
            Color::rgba(0.68, 0.68, 0.74, 1.0)
        } else {
            Color::rgba(0.36, 0.36, 0.42, 1.0)
        }
    }
    fn scrollbar(&self) -> Color {
        Color::rgba(0.5, 0.5, 0.6, 0.6)
    }

    /// Primary text ink for component labels/titles/values. A theme should override it, but the default follows the active light/dark mode rather than assuming light: a theme that overrides `surface` and forgets `ink` used to paint near-black text on its own dark panel.
    fn ink(&self) -> Color {
        if crate::mode::is_dark() {
            Color::rgba(0.98, 0.98, 1.0, 1.0)
        } else {
            Color::rgba(0.15, 0.15, 0.2, 1.0)
        }
    }
    /// The background a floating panel sits on — a menu, a dropdown, a dialog. Opaque by default, because the thing it covers must not read through it, and mode-following for the same reason as [`ink`](Self::ink).
    fn surface(&self) -> Color {
        if crate::mode::is_dark() {
            Color::rgba(0.09, 0.09, 0.11, 1.0)
        } else {
            Color::rgba(1.0, 1.0, 1.0, 1.0)
        }
    }
    /// A quiet, low-contrast surface tone for chip/tag backgrounds. Defaults to a faint neutral wash.
    fn surface_alt(&self) -> Color {
        Color::rgba(0.5, 0.5, 0.55, 0.1)
    }
    /// Hairline border/divider tone. Defaults to a faint neutral.
    fn border(&self) -> Color {
        Color::rgba(0.5, 0.5, 0.55, 0.35)
    }

    /// Semantic status colours. Defaults are conventional hues; a theme should override to match its palette.
    fn success(&self) -> Color {
        Color::rgba(0.4, 0.7, 0.4, 1.0)
    }
    fn warning(&self) -> Color {
        Color::rgba(0.9, 0.75, 0.4, 1.0)
    }
    fn error(&self) -> Color {
        Color::rgba(0.8, 0.35, 0.4, 1.0)
    }
    fn info(&self) -> Color {
        Color::rgba(0.4, 0.6, 0.8, 1.0)
    }

    /// Three progressively stronger highlight/elevation tints for hover, selection, and pressed states. Defaults to faint neutral washes a theme can override with palette-specific tones.
    fn highlight_low(&self) -> Color {
        Color::rgba(0.5, 0.5, 0.55, 0.06)
    }
    fn highlight_med(&self) -> Color {
        Color::rgba(0.5, 0.5, 0.55, 0.12)
    }
    fn highlight_high(&self) -> Color {
        Color::rgba(0.5, 0.5, 0.55, 0.20)
    }

    /// Not a token: where a theme hands over the values of tokens a plugin declares for itself, each keyed by its type and read back with [`use_theme_extension`]. The derive fills it from the fields marked `#[theme(extension)]`; a theme that inserts nothing leaves every plugin on its own defaults.
    fn register_extensions(&self, _extensions: &mut ThemeExtensions) {}
}

/// The values a theme supplies for vocabularies it does not define, one per type: a plugin declares its own token struct and the application's theme carries an instance of it, so neither has to name the other's type.
#[derive(Default)]
pub struct ThemeExtensions(HashMap<TypeId, Rc<dyn Any>>);

impl ThemeExtensions {
    /// Supplies `value` to every [`use_theme_extension::<K>`](use_theme_extension) under this theme, replacing an earlier value of the same type.
    pub fn insert<K: Default + Clone + 'static>(&mut self, value: K) {
        self.0.insert(TypeId::of::<K>(), Rc::new(value));
    }

    fn get<K: Clone + 'static>(&self) -> Option<K> {
        self.0.get(&TypeId::of::<K>())?.downcast_ref::<K>().cloned()
    }
}

/// One theme behind the three views it is read through: the catalogue asks it questions through `ThemeTokens`, `use_theme` hands the application its own type back, and a plugin reads its own tokens out of the extensions. `Rc<dyn Any>` is all the downcast needs, which is why a theme does not implement a trait to supply it.
///
/// The extensions are collected here, once per installed value, so they are replaced exactly when the theme is: a mode switch, a [`ScopedTheme::set`] and a hot reload's re-run of `setup` each install a new `Installed`, and no reader sees one theme's tokens beside another's extensions.
#[derive(Clone)]
pub(crate) struct Installed {
    theme: Rc<dyn Any>,
    tokens: Rc<dyn ThemeTokens>,
    extensions: Rc<ThemeExtensions>,
}

impl Installed {
    pub(crate) fn new<T: ThemeTokens + Clone + 'static>(theme: T) -> Self {
        let mut extensions = ThemeExtensions::default();
        theme.register_extensions(&mut extensions);
        let theme = Rc::new(theme);
        Self {
            theme: theme.clone(),
            tokens: theme,
            extensions: Rc::new(extensions),
        }
    }
}

thread_local! {
    // No `ManuallyDrop` and no TLS destructor: an `RwSignal` is an id with no destructor, so the slot is trivially droppable and `dlclose` stays safe. `reset_runtime` frees the storage.
    static THEME: RwSignal<Option<Installed>> = detached(|| signal(None));
}

/// Installs `theme` as the global default, for both [`use_theme`] and the catalogue's token reads, wherever no [`ScopedTheme`] is provided and the surface does not show a mode registered with a theme of its own.
pub fn set_theme<T: ThemeTokens + Clone + 'static>(theme: T) {
    install(Installed::new(theme));
}

pub(crate) fn install(installed: Installed) {
    THEME.with(|s| s.set(Some(installed)));
}

/// The theme that stands where no [`ScopedTheme`] is provided, subscribing the caller to it: the theme of the active surface's own mode where it has one registered with [`register_mode_theme`](crate::register_mode_theme), else the one [`set_theme`] installed.
fn global() -> Option<Installed> {
    crate::mode::surface_mode_theme().or_else(|| THEME.with(|s| s.get()))
}

/// A theme for one subtree, switchable in place: once [provided](Self::provide) to an owner, every read under that owner — including in effects, measures and handlers that re-enter it later — resolves this theme instead of the global one, and [`set`](Self::set) re-runs only the readers that resolved it.
///
/// A scope may also hold no theme of its own and resolve the global one, shadowing any provider above it: see [`follow_global`](Self::follow_global).
#[derive(Clone, Copy)]
pub struct ScopedTheme(RwSignal<Option<Installed>>);

impl ScopedTheme {
    /// A scoped theme belonging to the current owner, which is what frees it.
    pub fn new<T: ThemeTokens + Clone + 'static>(theme: T) -> Self {
        Self(signal(Some(Installed::new(theme))))
    }

    /// A scope that resolves whatever [`set_theme`] installs, as if no provider stood above it, until [`set`](Self::set) gives it a theme of its own. For a subtree that has to stay switchable between the application's theme and another one without being rebuilt. On a surface with a mode of its own, the global theme is that mode's, as it is with no provider at all.
    pub fn follow_global() -> Self {
        Self(signal(None))
    }

    /// A scoped theme holding the theme mode `id` was registered with through [`register_mode_theme`](crate::register_mode_theme), so a subtree shows that mode without it being installed for the application. `None` where no such theme is registered under `id`.
    pub fn for_mode(id: &str) -> Option<Self> {
        crate::mode::mode_theme_now(id).map(|installed| Self(signal(Some(installed))))
    }

    /// Swaps the theme, re-running whatever resolved this one.
    pub fn set<T: ThemeTokens + Clone + 'static>(&self, theme: T) {
        self.0.set(Some(Installed::new(theme)));
    }

    /// Drops the scope's own theme, so it resolves the global one again, as [`follow_global`](Self::follow_global) made it.
    pub fn clear(&self) {
        if self.0.peek_with(Option::is_some) {
            self.0.set(None);
        }
    }

    /// Whether the scope resolves the global theme rather than one of its own. Reactive.
    pub fn follows_global(&self) -> bool {
        self.0.with(Option::is_none)
    }

    /// This theme's tokens, read reactively.
    pub fn tokens(&self) -> Rc<dyn ThemeTokens> {
        match self.resolved() {
            Some(installed) => installed.tokens,
            None => DEFAULT_TOKENS.with(Rc::clone),
        }
    }

    /// The theme this scope stands for, subscribing the caller to it and, while it follows the global theme, to that.
    fn resolved(&self) -> Option<Installed> {
        match self.0.get() {
            Some(installed) => Some(installed),
            None => global(),
        }
    }

    /// Makes this the theme of everything built under the current owner, shadowing any provided above it.
    pub fn provide(self) {
        reactive_core::provide_context(Provided(self));
    }
}

impl<T: ThemeTokens + Clone + 'static> From<T> for ScopedTheme {
    fn from(theme: T) -> Self {
        Self::new(theme)
    }
}

/// A newtype so the key in an owner's context is this crate's alone.
struct Provided(ScopedTheme);

/// The nearest theme provided at or above the current owner, or `None` where only the global one applies.
pub fn nearest_theme() -> Option<ScopedTheme> {
    reactive_core::with_context::<Provided, _>(|provided| provided.0)
}

/// The theme in force here, subscribing the caller to exactly that one.
fn in_force() -> Option<Installed> {
    match nearest_theme() {
        Some(scoped) => scoped.resolved(),
        None => global(),
    }
}

/// A handle to the theme in force, read with `.get()` like any other reactive source.
///
/// `[view]` is where a theme is mostly read, and there a read has to happen inside whatever closure asks for it or it is the theme that was registered when the view was built, forever. A handle makes that the author's own spelling: `$theme.primary` is `theme.get().primary`, the same `$` that reads a signal, so the markup needs no rule of its own for what a theme read is — and the transpiler needs no branch for it.
///
/// Zero-sized and `Copy`, so it moves into any number of closures with nothing to clone.
pub struct Theme<T>(PhantomData<T>);

impl<T> Clone for Theme<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Theme<T> {}

impl<T> Default for Theme<T> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<T: Clone + 'static> Theme<T> {
    pub fn get(&self) -> T {
        use_theme::<T>()
    }
}

/// The theme in force as the application's own type: the nearest [`ScopedTheme`] above the current owner that holds a `T`, else the theme of the surface's own mode, else the global theme, walking past any of them that holds another type. A provider that [follows the global theme](ScopedTheme::follow_global) holds whatever the global theme holds. Reads reactively, so a switch re-runs the caller; panics if no provider or the global theme holds a `T`.
pub fn use_theme<T: Clone + 'static>() -> T {
    let scoped = reactive_core::find_context::<Provided, _>(|provided| {
        provided.0.resolved()?.theme.downcast_ref::<T>().cloned()
    });
    if let Some(theme) = scoped {
        return theme;
    }
    let held = |installed: &Installed| installed.theme.downcast_ref::<T>().cloned();
    crate::mode::surface_mode_theme()
        .and_then(|installed| held(&installed))
        .or_else(|| THEME.with(|s| s.with(|global| global.as_ref().and_then(held))))
        .unwrap_or_else(|| {
            panic!(
                "use_theme::<{}> found no theme of that type in force; install one with set_theme or a ScopedTheme",
                std::any::type_name::<T>()
            )
        })
}

/// Never `None`: with no theme registered the answer is the same table nobody overrode, because per-caller fallbacks drifted into a second palette that ignored light/dark mode.
pub fn use_theme_tokens() -> Rc<dyn ThemeTokens> {
    match in_force() {
        Some(installed) => installed.tokens,
        None => DEFAULT_TOKENS.with(Rc::clone),
    }
}

/// A plugin's own tokens as the theme in force supplies them, else `K::default()`.
///
/// Resolved like [`use_theme_tokens`]: the nearest provided theme, else the global one, and that theme only. A nested theme that supplies no `K` gives `K::default()` rather than an outer theme's `K`, so everything a component reads comes from one theme. Reads reactively, so a mode switch or a provider's [`set`](ScopedTheme::set) re-runs the caller.
pub fn use_theme_extension<K: Default + Clone + 'static>() -> K {
    in_force()
        .and_then(|installed| installed.extensions.get::<K>())
        .unwrap_or_default()
}

struct DefaultTokens;
impl ThemeTokens for DefaultTokens {}

thread_local! {
    static DEFAULT_TOKENS: Rc<dyn ThemeTokens> = Rc::new(DefaultTokens);
}

#[cfg(test)]
#[path = "context_test.rs"]
mod tests;
