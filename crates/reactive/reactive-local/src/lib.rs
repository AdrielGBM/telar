//! The per-surface thread-local swap, with no dependencies of its own.
//!
//! Split out of `reactive-core` so a crate wanting a swappable slot does not pull the reactive runtime in for
//! it — `platform-core` and `services-core` each carried a byte-for-byte copy of the macro rather than that.
//! `services-core` has since grown the dependency anyway, for the owner tree its context now lives in; the
//! split still earns its place for `platform-core`, and for this crate being where `detached` has to live.

mod detached;
#[macro_use]
mod surface_local;
pub mod reentry;

pub use detached::{detached, is_detached};
pub use surface_local::SurfaceSlot;
