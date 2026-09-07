//! Factory functions (`signal`, `effect`, `memo`) create nodes in the reactive graph. Struct constructors (`Runtime::new`, etc.) own their state. Free functions (`batch`, `set_flush_notify`) operate on the thread-local runtime.

#![warn(rustdoc::broken_intra_doc_links)]

mod effect;
mod memo;
mod reactive;
pub use reactive_local::reentry;
mod runtime;
mod signal;
mod source;
#[macro_use]

mod task;

pub use effect::{Effect, effect};
pub use memo::{Memo, memo};
pub use reactive::Reactive;
pub use reactive_local::{SurfaceSlot, detached, surface_local};
pub use runtime::{
    FlushNotifyHandle, OwnerGuard, OwnerId, SurfaceEnterGuard, SurfaceHandle, batch, begin_batch,
    context_provided_here, current_owner, current_surface, dispose_owner, dispose_surface_owners,
    end_batch, live_effect_count, live_signal_count, on_cleanup, owner_scope, provide_context,
    reset_runtime, set_current_surface, set_flush_notify, set_surface_enter_hook, with_context,
    with_owner,
};
pub use signal::{ReadSignal, RwSignal, signal};
pub use source::{Source, derive, derive_pair};
pub use task::{
    Emitter, Task, cancel_tasks_for, drain_tasks, reset_tasks, set_task_waker, spawn_stream,
    spawn_task,
};

#[cfg(test)]
#[path = "lib_test.rs"]
mod tests;
