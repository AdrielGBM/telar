//! A draw list as bytes, for a guest that keeps its own allocator: the guest encodes into a buffer it owns and lends, and the host decodes into commands it owns.
//!
//! Two limits of what crosses, both by design. A link to a scheme the host's URI policy refuses arrives as no link, and the box or run that carried it still draws. An image whose pixels live in a GPU texture the guest made cannot cross, since the texture belongs to the guest's device, and arrives as an image of its size with no pixels.

mod fonts;
mod frame;
mod intern;
mod value;

pub use frame::{FrameDecoder, FrameEncoder};
pub use wire_core::{WIRE_VERSION, WireError};

/// Why a lent frame could not be decoded.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FrameError {
    #[error(transparent)]
    Malformed(#[from] WireError),
    #[error("not a draw list")]
    NotAFrame,
    #[error("draw list version {found}, expected {WIRE_VERSION}")]
    Version { found: u16 },
    #[error("slot {slot} names nothing")]
    UnknownSlot { slot: usize },
    #[error("frame {found} follows {last:?}, which the encoder did not build it on")]
    OutOfStep { last: Option<u64>, found: u64 },
    #[error("the guest lent no pixels for image {0:#x}")]
    MissingImage(u64),
    #[error("the pixels lent for image {0:#x} do not hash to it")]
    ImageMismatch(u64),
}
