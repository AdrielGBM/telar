use std::any::Any;
use std::rc::Rc;

use reactive_core::{RwSignal, create_rw_signal};

pub trait Theme: 'static {}

thread_local! {
    static THEME: RwSignal<Rc<dyn Any>> = create_rw_signal(Rc::new(()) as Rc<dyn Any>);
}

pub fn use_theme<T: Theme + Clone>() -> T {
    THEME.with(|s| {
        s.get()
            .downcast::<T>()
            .unwrap_or_else(|_| {
                panic!(
                    "use_theme::<{}> called but no theme of that type has been set",
                    std::any::type_name::<T>()
                )
            })
            .as_ref()
            .clone()
    })
}

pub fn set_theme<T: Theme>(theme: T) {
    THEME.with(|s| s.set(Rc::new(theme) as Rc<dyn Any>));
}
