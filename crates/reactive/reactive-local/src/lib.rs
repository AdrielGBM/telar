//! The per-surface thread-local swap, with no dependencies of its own.
//!
//! Split out of `reactive-core` because the two other crates that need it cannot depend on that: `services-core`
//! and `platform-core` each carried a byte-for-byte copy of the macro rather than pull in slab, smallvec and
//! rustc-hash for a slot that needs none of them.

mod detached;
#[macro_use]
mod surface_local;
pub mod reentry;

pub use detached::{detached, is_detached};
pub use surface_local::SurfaceSlot;
