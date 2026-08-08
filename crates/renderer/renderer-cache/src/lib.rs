//! One cache, and one place where its bounds are written down.
//!
//! Telar's renderers used to hold around twenty caches between them, and the count was inflated by caches that
//! duplicated each other or that belonged to a backend other than the one paying for them: the CPU backend stored
//! every text raster twice, once as bytes and once as a pixmap; the pixmap copy had no byte bound and no admission
//! rule, so it silently undid the admission the byte copy applied; and the GPU backend hand-rolled the same
//! map-plus-eviction-queue four times, each with its own handling of entries re-touched since they were queued, and
//! bounded two of them by age with no size ceiling at all.
//!
//! What replaces them is [`Cache`] — a weight-budgeted LRU that also evicts by idle age and can require a second
//! sighting before it keeps anything — parameterized by a [`Policy`] and a weigh function rather than specialized
//! per backend. The bounds themselves live in [`limits`], together, because comparing them is what revealed the
//! problem in the first place.
//!
//! ```
//! use telar_renderer_cache::{Cache, limits};
//!
//! let mut rasters: Cache<u64, Vec<u8>> = Cache::new(limits::TEXT_RASTER, Vec::len);
//! if rasters.get(&7).is_none() {
//!     rasters.insert(7, vec![0; 4096]);
//! }
//! ```

mod cache;
pub mod limits;
mod policy;
pub mod registry;

pub use cache::{Cache, CacheStat};
pub use policy::Policy;
