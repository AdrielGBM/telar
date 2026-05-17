use std::rc::Rc;

use crate::runtime;

pub struct Effect {
    id: usize,
}

impl Drop for Effect {
    fn drop(&mut self) {
        runtime::deregister_effect(self.id);
    }
}

pub fn create_effect(f: impl Fn() + 'static) -> Effect {
    let id = runtime::register_effect(Rc::new(f));
    runtime::run_effect(id);
    Effect { id }
}
