use crate::runtime;

/// A live subscription. Dropping it deregisters the effect, so the closure runs once and never again — which
/// looks exactly like a working binding until the value it derives is expected to move. Bind it to something
/// that lives as long as the work should: a struct field, a returned value, or a `let` the reader captures.
#[must_use = "dropping the handle deregisters the effect: it runs once and then stops. Bind it (a struct \
              field, a returned value, a captured `let`) for as long as the subscription should last, or use \
              `memo`, whose handle is `Rc`-backed and kept alive by whoever reads it."]
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
