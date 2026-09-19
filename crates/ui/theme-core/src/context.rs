//! The installed theme: setting one, and reading it back as either the app's own type or the token trait.

use std::any::Any;
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
    fn primary(&self) -> Color {
        Color::rgba(0.24, 0.47, 0.98, 1.0)
    }
    fn on_primary(&self) -> Color {
        Color::rgba(1.0, 1.0, 1.0, 1.0)
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

    fn muted(&self) -> Color {
        Color::rgba(0.5, 0.5, 0.6, 0.6)
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
}

/// One theme behind the two views it is read through: the catalogue asks it questions through `ThemeTokens`, while `use_theme` hands the application its own type back. `Rc<dyn Any>` is all the downcast needs, which is why a theme does not implement a trait to supply it.
#[derive(Clone)]
struct Installed {
    theme: Rc<dyn Any>,
    tokens: Rc<dyn ThemeTokens>,
}

impl Installed {
    fn new<T: ThemeTokens + Clone + 'static>(theme: T) -> Self {
        let theme = Rc::new(theme);
        Self {
            theme: theme.clone(),
            tokens: theme,
        }
    }
}

thread_local! {
    // No `ManuallyDrop` and no TLS destructor: an `RwSignal` is an id with no destructor, so the slot is trivially droppable and `dlclose` stays safe. `reset_runtime` frees the storage.
    static THEME: RwSignal<Option<Installed>> = detached(|| signal(None));
}

/// Installs `theme` as the global default, for both [`use_theme`] and the catalogue's token reads, wherever no [`ScopedTheme`] is provided.
pub fn set_theme<T: ThemeTokens + Clone + 'static>(theme: T) {
    THEME.with(|s| s.set(Some(Installed::new(theme))));
}

/// A theme for one subtree, switchable in place: once [provided](Self::provide) to an owner, every read under that owner — including in effects, measures and handlers that re-enter it later — resolves this theme instead of the global one, and [`set`](Self::set) re-runs only the readers that resolved it.
#[derive(Clone, Copy)]
pub struct ScopedTheme(RwSignal<Installed>);

impl ScopedTheme {
    /// A scoped theme belonging to the current owner, which is what frees it.
    pub fn new<T: ThemeTokens + Clone + 'static>(theme: T) -> Self {
        Self(signal(Installed::new(theme)))
    }

    /// Swaps the theme, re-running whatever resolved this one.
    pub fn set<T: ThemeTokens + Clone + 'static>(&self, theme: T) {
        self.0.set(Installed::new(theme));
    }

    /// This theme's tokens, read reactively.
    pub fn tokens(&self) -> Rc<dyn ThemeTokens> {
        self.0.with(|installed| Rc::clone(&installed.tokens))
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
        Some(scoped) => Some(scoped.0.get()),
        None => THEME.with(|s| s.get()),
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

/// The theme in force as the application's own type: the nearest [`ScopedTheme`] above the current owner that holds a `T`, else the global theme, walking past providers of other types. Reads reactively, so a switch re-runs the caller; panics if no provider or the global theme holds a `T`.
pub fn use_theme<T: Clone + 'static>() -> T {
    let scoped = reactive_core::find_context::<Provided, _>(|provided| {
        provided
            .0
            .0
            .with(|installed| installed.theme.downcast_ref::<T>().cloned())
    });
    if let Some(theme) = scoped {
        return theme;
    }
    THEME
        .with(|s| s.with(|global| global.as_ref()?.theme.downcast_ref::<T>().cloned()))
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

struct DefaultTokens;
impl ThemeTokens for DefaultTokens {}

thread_local! {
    static DEFAULT_TOKENS: Rc<dyn ThemeTokens> = Rc::new(DefaultTokens);
}

#[cfg(test)]
#[path = "context_test.rs"]
mod tests;
