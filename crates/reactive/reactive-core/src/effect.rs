//! [`effect`]: a closure re-run whenever a signal it read changes.

use crate::runtime;

/// Runs `f` now, and again whenever a signal it read changes, for as long as the owner active at this call lives: disposing that owner stops it. To stop an effect on its own, run it inside an [`owner_scope`](crate::owner_scope) and dispose that scope.
pub fn effect(f: impl Fn() + 'static) {
    let id = runtime::register_effect(Box::new(f));
    runtime::run_effect(id);
}
