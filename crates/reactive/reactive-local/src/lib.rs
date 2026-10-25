//! The per-surface thread-local swap, thread-locals that refuse a destructor, freeing what a thread-local holds when its thread exits, and the hook every thread the runtime starts goes through, with no dependencies of its own.
//!
//! Split out of `reactive-core` so a crate wanting a swappable slot does not pull the reactive runtime in for it — `platform-core` and `services-core` each carried a byte-for-byte copy of the macro rather than that. `services-core` has since grown the dependency anyway, for the owner tree its context now lives in; the split still earns its place for `platform-core`, and for this crate being where `detached` has to live.

#![warn(rustdoc::broken_intra_doc_links)]

mod detached;
mod dropless;
#[macro_use]
mod surface_local;
pub mod reentry;
pub mod thread_exit;
pub mod threads;

pub use detached::{detached, is_detached};
pub use surface_local::SurfaceSlot;
