//! [`ElementId`] and [`SurfaceKey`]: the names a frame gives its boxes and the surfaces drawn inside it.

use std::num::NonZeroU64;

/// Identifies one box across frames.
///
/// The layout node's own id: it is created with the widget, lives as long as it, and is already the thing every other layer uses to name that box. Minting a second identity would only be one more thing to keep in step.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct ElementId(pub u64);

/// Names a surface drawn inside another's frame, as the runtime that drew the frame numbers its surfaces.
///
/// Each surface numbers its boxes and its controls from its own counters, so an id read inside a nested surface means something only beside the key of that surface. The key is what the runtime's surface handle carries, made a plain value a frame can hold on any thread; it names that surface only in the runtime that drew the frame.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct SurfaceKey(pub NonZeroU64);
