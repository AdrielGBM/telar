//! A draw list as bytes, for a guest that keeps its own allocator: the guest encodes into a buffer it owns and lends, and the host decodes into commands it owns.

mod bytes;
mod frame;
mod intern;
mod value;

pub use frame::{FrameDecoder, FrameEncoder};

/// Bumped whenever the spelling of any value changes, since host and guest may be built from different sources.
pub const WIRE_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WireError {
    #[error("the frame ends early")]
    Truncated,
    #[error("not a draw list")]
    NotAFrame,
    #[error("draw list version {found}, expected {WIRE_VERSION}")]
    Version { found: u16 },
    #[error("unknown {of} tag {tag}")]
    Tag { of: &'static str, tag: u8 },
    #[error("invalid {0}")]
    Invalid(&'static str),
    #[error("slot {slot} names nothing")]
    UnknownSlot { slot: usize },
    #[error("frame {found} follows {last:?}, which the encoder did not build it on")]
    OutOfStep { last: Option<u64>, found: u64 },
    #[error("the guest lent no pixels for image {0:#x}")]
    MissingImage(u64),
    #[error("the pixels lent for image {0:#x} do not hash to it")]
    ImageMismatch(u64),
    #[error("{0} bytes left after the last command")]
    Trailing(usize),
}
