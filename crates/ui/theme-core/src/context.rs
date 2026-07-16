use std::mem::ManuallyDrop;
use std::rc::Rc;

use geometry_core::Color;
use reactive_core::{RwSignal, signal};

// Flexible theme contract: users define their own tokens with whatever names they want. `as_any` is the only requirement so `use_theme` can downcast back to the concrete type.
pub trait Theme: 'static {
    fn as_any(&self) -> &dyn std::any::Any;
}

// Opt-in semantic-token contract the built-in component catalogue reads through. A theme implements this so a component can resolve semantic colors without knowing the concrete theme type. Only the two primary tokens are mandatory; the rest carry defaults so a theme can omit them.
pub trait ThemeTokens: 'static {
    fn primary(&self) -> Color;
    fn on_primary(&self) -> Color;

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
