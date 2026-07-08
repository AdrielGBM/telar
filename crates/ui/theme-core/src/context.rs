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
}

thread_local! {
    // ManuallyDrop suppresses RwSignal's Drop impl so no TLS destructor is registered. Cleanup happens via reset_runtime() which drops the entire Runtime (and its signals slab).
    static THEME: ManuallyDrop<RwSignal<Option<Rc<dyn Theme>>>> =
        ManuallyDrop::new(signal(None));
    static THEME_TOKENS: ManuallyDrop<RwSignal<Option<Rc<dyn ThemeTokens>>>> =
        ManuallyDrop::new(signal(None));
}

// Installs a theme that also drives the built-in component catalogue. The same value is stored behind both trait objects so `use_theme` and `use_theme_tokens` stay in sync.
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

// Returns the semantic tokens so type-agnostic built-in components can read theme colors. `None` when the installed theme does not implement `ThemeTokens`.
pub fn use_theme_tokens() -> Option<Rc<dyn ThemeTokens>> {
    THEME_TOKENS.with(|s| s.get())
}
