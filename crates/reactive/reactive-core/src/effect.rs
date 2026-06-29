use crate::runtime;

pub struct Effect {
    id: runtime::EffectId,
}

impl Drop for Effect {
    fn drop(&mut self) {
        runtime::deregister_effect(self.id);
    }
}

pub fn effect(f: impl Fn() + 'static) -> Effect {
    let id = runtime::register_effect(Box::new(f));
    runtime::run_effect(id);
    Effect { id }
}
