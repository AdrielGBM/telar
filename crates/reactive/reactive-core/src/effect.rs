use std::rc::Rc;

use crate::runtime;

pub struct Effect {
    _id: usize,
}

pub fn create_effect(f: impl Fn() + 'static) -> Effect {
    let id = runtime::register_effect(Rc::new(f));
    runtime::run_effect(id);
    Effect { _id: id }
}
