use std::any::Any;
use std::rc::Rc;

use reactive_core::{RwSignal, create_rw_signal};

thread_local! {
    static THEME: RwSignal<Rc<dyn Any>> = create_rw_signal(Rc::new(()) as Rc<dyn Any>);
}

pub fn use_theme<T: 'static>() -> Rc<T> {
    THEME.with(|s| {
        s.get().downcast::<T>().unwrap_or_else(|_| {
            panic!(
                "use_theme::<{}> called but no theme of that type has been set",
                std::any::type_name::<T>()
            )
        })
    })
}

pub fn set_theme<T: 'static>(theme: T) {
    THEME.with(|s| s.set(Rc::new(theme) as Rc<dyn Any>));
}
