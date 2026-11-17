//! [`effect`]: a closure re-run whenever a signal it read changes.

use crate::runtime;

/// Runs `f` now, and again whenever a signal it read changes, for as long as the owner active at this call lives: disposing that owner stops it. To stop an effect on its own, run it inside an [`owner_scope`](crate::owner_scope) and dispose that scope.
pub fn effect(f: impl Fn() + 'static) {
    let id = runtime::register_effect(Box::new(f));
    runtime::run_effect(id);
}

/// Runs `f` without following what it reads: inside an effect or a view, a value read once on purpose.
pub fn untrack<R>(f: impl FnOnce() -> R) -> R {
    runtime::untracked(f)
}

/// Runs `f` outside any effect, answering what it returned and whether it read any signal on the way: whether the value has to be worked out again when what it read changes, or never can change.
pub fn reads_state<R>(f: impl FnOnce() -> R) -> (R, bool) {
    runtime::reads_state(f)
}

#[cfg(test)]
#[path = "effect.test.rs"]
mod tests;
