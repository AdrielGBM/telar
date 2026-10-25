//! Factory functions (`signal`, `effect`, `memo`) create nodes in the reactive graph. Struct constructors (`Runtime::new`, etc.) own their state. Free functions (`batch`, `set_flush_notify`) operate on the thread-local runtime.

#![warn(rustdoc::broken_intra_doc_links)]

mod effect;
mod memo;
mod reactive;
pub use reactive_local::{reentry, thread_exit, threads};
mod runtime;
mod signal;
mod source;
#[macro_use]
mod surface_scoped;
mod transaction;
#[macro_use]
mod task;
mod timer;

pub use effect::{Effect, effect};
pub use memo::{Memo, memo};
pub use reactive::Reactive;
pub use reactive_local::{
    SurfaceSlot, detached, dropless_thread_local, released_static, surface_local,
};
#[cfg(test)]
use runtime::set_current_surface;
pub use runtime::{
    FlushNotifyHandle, OpenBatch, OwnerGuard, OwnerId, PanicCatch, PanicPayload, SurfaceHandle,
    after_settle, batch, context_provided_here, current_owner, current_surface, dispose_owner,
    dispose_surface, dispose_surface_owners, find_context, in_surface_world, live_effect_count,
    live_owner_count, live_signal_count, on_cleanup, owner_scope, owner_within, provide_context,
    reset_runtime, root_scope, set_flush_notify, set_panic_catch, set_surface_enter_hook,
    with_context, with_current_surface, with_owner,
};
pub use signal::{ReadSignal, RwSignal, signal};
pub use source::{Source, derive, derive_pair};
pub use surface_scoped::SurfaceScoped;
pub use task::{
    Emitter, Task, cancel_tasks_for, drain_tasks, reset_tasks, set_task_waker, spawn_stream,
    spawn_task,
};
pub use timer::{
    Timer, advance_timer_clock, cancel_timers_for, fire_timers, next_timer_due, reset_timers,
    run_after, timer_now, until_next_timer,
};
pub use transaction::{Transaction, TransactionError};

#[cfg(test)]
#[path = "lib_test.rs"]
mod tests;

#[cfg(doctest)]
#[path = "thread_bound_test.rs"]
mod thread_bound_tests;
