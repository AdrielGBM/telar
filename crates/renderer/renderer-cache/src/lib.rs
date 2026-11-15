//! One cache, and one place where its bounds are written down.
//!
//! Every renderer cache is a [`Cache`] — a weight-budgeted LRU that also evicts by idle age and can require a second sighting before it keeps anything — parameterized by a [`Policy`] and a weigh function rather than specialized per backend, so each backend keeps one copy of what it draws and every copy has a byte bound. The bounds themselves live in [`limits`], together, because they can only be judged against each other.
//!
//! ```
//! use telar_renderer_cache::{Cache, limits};
//!
//! let mut rasters: Cache<u64, Vec<u8>> = Cache::new(limits::TEXT_RASTER, Vec::len);
//! if rasters.get(&7).is_none() {
//!     rasters.insert(7, vec![0; 4096]);
//! }
//! ```

#![warn(missing_docs)]
#![warn(rustdoc::broken_intra_doc_links)]

mod cache;
pub mod limits;
mod policy;
pub mod registry;

pub use cache::{Cache, CacheStat, Fetched};
pub use policy::Policy;
