use std::mem::ManuallyDrop;
use std::rc::Rc;

use reactive_core::{RwSignal, signal};
use renderer_core::Color;

// Flexible theme contract: users define their own tokens with whatever names they want. `as_any` is the only requirement so `use_theme` can downcast back to the concrete type.
pub trait Theme: 'static {
    fn as_any(&self) -> &dyn std::any::Any;
}

// Opt-in contract that built-in widgets read through. A theme implements this so widgets can resolve semantic colors without knowing the concrete theme type. Only the two primary tokens are mandatory; muted/scrollbar carry defaults so a theme can omit them.
pub trait WidgetTheme: 'static {
    fn widget_primary(&self) -> Color;
    fn widget_on_primary(&self) -> Color;

    fn widget_muted(&self) -> Color {
        Color::rgba(0.5, 0.5, 0.6, 0.6)
    }
    fn widget_scrollbar(&self) -> Color {
        Color::rgba(0.5, 0.5, 0.6, 0.6)
    }
}

thread_local! {
    // ManuallyDrop suppresses RwSignal's Drop impl so no TLS destructor is registered. Cleanup happens via reset_runtime() which drops the entire Runtime (and its signals slab).
    static THEME: ManuallyDrop<RwSignal<Option<Rc<dyn Theme>>>> =
        ManuallyDrop::new(signal(None));
    static WIDGET_THEME: ManuallyDrop<RwSignal<Option<Rc<dyn WidgetTheme>>>> =
        ManuallyDrop::new(signal(None));
}

// Installs a theme that also drives built-in widgets. The same value is stored behind both trait objects so `use_theme` and `use_widget_theme` stay in sync.
pub fn set_theme_with_widgets<T: Theme + WidgetTheme + Clone + 'static>(theme: T) {
    let theme = Rc::new(theme);
    let as_theme: Rc<dyn Theme> = theme.clone();
    let as_widget: Rc<dyn WidgetTheme> = theme;
    THEME.with(|s| s.set(Some(as_theme)));
    WIDGET_THEME.with(|s| s.set(Some(as_widget)));
}

pub fn use_theme<T: Theme + Clone + 'static>() -> T {
    THEME.with(|s| {
        let theme = s.get().unwrap_or_else(|| {
            panic!(
                "use_theme::<{}> called but no theme has been set; call set_theme_with_widgets first",
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

// Returns the widget theme so type-agnostic built-in widgets can read semantic colors. `None` when the installed theme does not implement `WidgetTheme`.
pub fn use_widget_theme() -> Option<Rc<dyn WidgetTheme>> {
    WIDGET_THEME.with(|s| s.get())
}
