use std::any::Any;
use std::mem::ManuallyDrop;
use std::rc::Rc;

use geometry_core::Color;
use reactive_core::{RwSignal, detached, signal};
use renderer_core::Declared;

/// Opt-in semantic-token contract the built-in component catalogue reads through, so a component can resolve a
/// token without knowing the concrete theme type.
///
/// **Every method carries a default**, which makes `impl ThemeTokens for MyTheme {}` valid and each token an
/// independent opt-in: a theme answers the questions it cares about and lets the catalogue keep its own answer
/// for the rest. This trait is deliberately not where a theme's vocabulary lives — that belongs to the theme's
/// own type, reachable in full through [`use_theme`]. What is here is only the subset a component written
/// without knowledge of that type has to be able to ask for.
///
/// The metric tokens are *bases*, not a size scale: a component derives its own proportions from one rather
/// than asking for a named role, because naming the roles would decide for every application which roles may
/// exist. One number scales the thing; the component keeps its own ratios.
///
/// Nothing here answers what size the text is, and that is the point. Text size inherits, so it is not a
/// question a component asks a theme — it is one it asks the region it is standing in. A theme that wants to
/// move it declares it once, at [`root`](Self::root).
pub trait ThemeTokens: 'static {
    fn primary(&self) -> Color {
        Color::rgba(0.24, 0.47, 0.98, 1.0)
    }
    fn on_primary(&self) -> Color {
        Color::rgba(1.0, 1.0, 1.0, 1.0)
    }

    /// Base corner radius in px. A component rounds by this, or by a step of the scale below where its shape
    /// asks for one (a pill is not a card).
    fn radius(&self) -> f32 {
        4.0
    }

    /// The steps either side of [`radius`](Self::radius), so a theme owns **how round everything is** instead
    /// of each component keeping its own literal.
    ///
    /// This is the axis an application actually restyles, and a scale of three steps derived from one base is
    /// what a design system needs to be reachable from outside. A component that hardcodes
    /// `BorderRadius::all(8.0)` is not themeable at all — the caller can change the base radius and watch
    /// nothing move — and the fix is not a prop per component but a token they all read.
    ///
    /// A theme that wants a flat scale returns the same number from all three; one that wants a rounder
    /// language moves the base and the steps follow.
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
    /// Every property of a [`Declared`] is an *inheriting* one, which makes this the only honest place for a
    /// theme to set one: said here it is a property of the document that anything below can override, rather
    /// than an answer each component has to remember to ask for. Said as a token it would be a second channel
    /// for a value the cascade already carries, and the two disagree the moment a region declares its own —
    /// which is how a hint ends up in the theme's near-black inside a panel written in white.
    ///
    /// This is also the whole of a theme's typography. Eight of the ten inherited properties never had a token
    /// at all, so a theme with a face or a leading of its own had to write it at every call site.
    fn root(&self) -> Declared {
        Declared::default()
    }
    /// Default size of a standalone icon in px.
    fn icon_size(&self) -> f32 {
        16.0
    }

    /// The steps either side of [`spacing`](Self::spacing), so a theme owns **how much air everything has**
    /// instead of each component keeping its own literal.
    ///
    /// Note the base sits in the *middle* here, where the radius base is the largest step: "how round is the
    /// biggest thing" and "what is the default gap" are different questions, and a scale that pretended
    /// otherwise would make every component either cramped or airy the moment a theme moved one number.
    ///
    /// A theme that wants a flat rhythm returns the same number from all four; one that wants a roomier
    /// language moves the base and the steps follow.
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

    /// Primary text ink for component labels/titles/values. A theme should override it, but the default
    /// follows the active light/dark mode rather than assuming light: a theme that overrides `surface` and
    /// forgets `ink` used to paint near-black text on its own dark panel.
    fn ink(&self) -> Color {
        if crate::mode::is_dark() {
            Color::rgba(0.98, 0.98, 1.0, 1.0)
        } else {
            Color::rgba(0.15, 0.15, 0.2, 1.0)
        }
    }
    /// The background a floating panel sits on — a menu, a dropdown, a dialog. Opaque by default, because the
    /// thing it covers must not read through it, and mode-following for the same reason as [`ink`](Self::ink).
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

    /// Three progressively stronger highlight/elevation tints for hover, selection, and pressed states.
    /// Defaults to faint neutral washes a theme can override with palette-specific tones.
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

thread_local! {
    // ManuallyDrop suppresses RwSignal's Drop impl so no TLS destructor is registered. Cleanup happens via reset_runtime() which drops the entire Runtime (and its signal arena).
    // The same value behind two views: the catalogue asks it questions through `ThemeTokens`, and `use_theme` hands the application its own type back. `Rc<dyn Any>` is the whole of what the downcast needs, which is why a theme no longer implements a trait to supply it.
    static THEME: ManuallyDrop<RwSignal<Option<Rc<dyn Any>>>> = ManuallyDrop::new(detached(|| signal(None)));
    static THEME_TOKENS: ManuallyDrop<RwSignal<Option<Rc<dyn ThemeTokens>>>> =
        ManuallyDrop::new(detached(|| signal(None)));
}

pub fn set_theme<T: ThemeTokens + Clone + 'static>(theme: T) {
    let theme = Rc::new(theme);
    THEME.with(|s| s.set(Some(theme.clone() as Rc<dyn Any>)));
    THEME_TOKENS.with(|s| s.set(Some(theme as Rc<dyn ThemeTokens>)));
}

pub fn use_theme<T: Clone + 'static>() -> T {
    THEME.with(|s| {
        let theme = s.get().unwrap_or_else(|| {
            panic!(
                "use_theme::<{}> called but no theme has been set; call set_theme first",
                std::any::type_name::<T>()
            )
        });
        theme
            .downcast_ref::<T>()
            .unwrap_or_else(|| {
                panic!(
                    "use_theme::<{}> called but a theme of a different type is set",
                    std::any::type_name::<T>()
                )
            })
            .clone()
    })
}

/// The tokens in force: the registered theme, or the trait's own answers when nothing is registered.
///
/// Not an `Option`, because no theme registered is not a different set of values — it is this same table
/// with nobody having overridden it. Every caller that had to handle the `None` supplied a fallback of its
/// own, and those fallbacks became a second palette that drifted: the focus ring's accent and
/// [`primary`](ThemeTokens::primary) were different blues, and [`ink`](ThemeTokens::ink) and
/// [`surface`](ThemeTokens::surface) follow the light/dark mode here while the constants standing in for
/// them did not — so an application that registered no theme could not reach the mode-following default at
/// all.
pub fn use_theme_tokens() -> Rc<dyn ThemeTokens> {
    match THEME_TOKENS.with(|s| s.get()) {
        Some(tokens) => tokens,
        None => DEFAULT_TOKENS.with(Rc::clone),
    }
}

struct DefaultTokens;
impl ThemeTokens for DefaultTokens {}

thread_local! {
    static DEFAULT_TOKENS: Rc<dyn ThemeTokens> = Rc::new(DefaultTokens);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole reason this is not an `Option`: every caller that had to handle a missing theme wrote its
    /// own flat constant, and none of them followed the mode — so the careful mode-following default was
    /// unreachable on exactly the path that runs when nobody has configured anything.
    #[test]
    fn an_unregistered_theme_still_follows_the_mode() {
        crate::register_mode("light", || {});
        crate::register_mode("dark", || {});
        crate::follow_system("light", "dark");

        crate::set_system_dark(false);
        let light_ink = use_theme_tokens().ink();
        crate::set_system_dark(true);
        let dark_ink = use_theme_tokens().ink();

        assert!(
            light_ink.r < 0.5,
            "dark ink on a light page, got {light_ink:?}"
        );
        assert!(
            dark_ink.r > 0.5,
            "light ink on a dark page, got {dark_ink:?}"
        );
        crate::set_system_dark(false);
    }

    #[derive(Clone)]
    struct Blue;
    impl ThemeTokens for Blue {
        fn primary(&self) -> Color {
            Color::rgba(0.0, 0.0, 1.0, 1.0)
        }
    }

    /// A theme answering one question keeps the table's answers for the rest, which is what makes
    /// `impl ThemeTokens for MyTheme {}` a valid theme.
    #[test]
    fn a_registered_theme_overrides_only_what_it_answers() {
        set_theme(Blue);
        assert_eq!(
            use_theme_tokens().primary(),
            Color::rgba(0.0, 0.0, 1.0, 1.0)
        );
        assert_eq!(use_theme_tokens().radius(), DefaultTokens.radius());
        THEME_TOKENS.with(|s| s.set(None));
        THEME.with(|s| s.set(None));
    }
}
