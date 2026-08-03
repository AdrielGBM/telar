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

    /// Base corner radius in px. A component rounds by this, or by a multiple of it where its shape asks for
    /// one (a pill is not a card).
    fn radius(&self) -> f32 {
        4.0
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

    fn muted(&self) -> Color {
        Color::rgba(0.5, 0.5, 0.6, 0.6)
    }
    fn scrollbar(&self) -> Color {
        Color::rgba(0.5, 0.5, 0.6, 0.6)
    }

    /// Primary text ink for component labels/titles/values. Defaults to a near-black; a theme should override
    /// it (e.g. a dark theme returns a light ink) so component text stays legible on its surface.
    fn ink(&self) -> Color {
        Color::rgba(0.15, 0.15, 0.2, 1.0)
    }
    /// The background a floating panel sits on — a menu, a dropdown, a dialog. Opaque by default, because the
    /// thing it covers must not read through it.
    fn surface(&self) -> Color {
        Color::rgba(1.0, 1.0, 1.0, 1.0)
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
