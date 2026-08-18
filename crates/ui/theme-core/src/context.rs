use std::mem::ManuallyDrop;
use std::rc::Rc;

use geometry_core::Color;
use reactive_core::{RwSignal, signal};

// Flexible theme contract: users define their own tokens with whatever names they want. `as_any` is the only requirement so `use_theme` can downcast back to the concrete type.
pub trait Theme: 'static {
    fn as_any(&self) -> &dyn std::any::Any;
}

/// Opt-in semantic-token contract the built-in component catalogue reads through, so a component can resolve a
/// token without knowing the concrete theme type.
///
/// **Every method carries a default**, which makes `impl ThemeTokens for MyTheme {}` valid and each token an
/// independent opt-in: a theme answers the questions it cares about and lets the catalogue keep its own answer
/// for the rest. This trait is deliberately not where a theme's vocabulary lives — that belongs to the theme's
/// own type, reachable in full through [`use_theme`]. What is here is only the subset a component written
/// without knowledge of that type has to be able to ask for.
///
/// The metric tokens are *bases*, not a size scale. A catalogue component derives its own proportions from
/// [`font_size`](Self::font_size) rather than asking for a named role, because naming the roles would decide for
/// every application which roles may exist. One number scales the type; the component keeps its own ratios.
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
    /// Base body text size in px. Every catalogue component scales its own text off this, so changing it scales
    /// the whole type ramp.
    fn font_size(&self) -> f32 {
        14.0
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
    // ManuallyDrop suppresses RwSignal's Drop impl so no TLS destructor is registered. Cleanup happens via reset_runtime() which drops the entire Runtime (and its signals slab).
    static THEME: ManuallyDrop<RwSignal<Option<Rc<dyn Theme>>>> =
        ManuallyDrop::new(signal(None));
    static THEME_TOKENS: ManuallyDrop<RwSignal<Option<Rc<dyn ThemeTokens>>>> =
        ManuallyDrop::new(signal(None));
}

pub fn set_theme<T: Theme + ThemeTokens + Clone + 'static>(theme: T) {
    let theme = Rc::new(theme);
    let as_theme: Rc<dyn Theme> = theme.clone();
    let as_tokens: Rc<dyn ThemeTokens> = theme;
    THEME.with(|s| s.set(Some(as_theme)));
    THEME_TOKENS.with(|s| s.set(Some(as_tokens)));
}

pub fn use_theme<T: Theme + Clone + 'static>() -> T {
    THEME.with(|s| {
        let theme = s.get().unwrap_or_else(|| {
            panic!(
                "use_theme::<{}> called but no theme has been set; call set_theme first",
                std::any::type_name::<T>()
            )
        });
        theme
            .as_any()
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

pub fn use_theme_tokens() -> Option<Rc<dyn ThemeTokens>> {
    THEME_TOKENS.with(|s| s.get())
}
