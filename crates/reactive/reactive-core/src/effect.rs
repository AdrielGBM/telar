//! [`Effect`]: a closure re-run whenever a signal it read changes.

use crate::runtime;

/// A live subscription.
///
/// The handle is inert: an effect belongs to the owner that was active when it was registered, and stops when that owner is disposed. Dropping this changes nothing, so binding it is a choice about readability rather than a requirement — which is what removes the trap it used to carry, where `let _ = effect(…)` ran the closure once and then silently never again.
#[derive(Clone, Copy)]
pub struct Effect {
    #[allow(dead_code)]
    id: runtime::EffectId,
}

/// Runs `f` now, and again whenever a signal it read changes. The returned handle owns the subscription.
pub fn effect(f: impl Fn() + 'static) -> Effect {
    let id = runtime::register_effect(Box::new(f));
    runtime::run_effect(id);
    Effect { id }
}
