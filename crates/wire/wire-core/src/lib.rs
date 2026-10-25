//! The byte spelling of every value that crosses between a Telar host and a guest library.
//!
//! A guest links its own copy of std and its own allocator, so nothing it owns may be handed to the host to keep or free, nor the other way round. What crosses is bytes instead: one side spells a value into a buffer it owns and lends it, and the other reads it back into values it allocated itself. Rich values travel this way, as WebRender's display lists do, and only the handful of `repr(C)` building blocks in `platform-core::guest` cross as themselves.
//!
//! Each type's spelling lives beside the type, in the crate that defines it, through [`Wire`]; this crate holds the primitives they are spelled in and the spelling of the standard types they are built from. Every enum names its tags out loud, so the bytes never follow declaration order and a reordered variant changes nothing.

#![forbid(unsafe_code)]
#![warn(rustdoc::broken_intra_doc_links)]

use std::fmt;

mod bytes;
#[cfg(any(test, feature = "golden"))]
mod golden;
mod value;

pub use bytes::{Reader, Writer};
#[cfg(any(test, feature = "golden"))]
pub use golden::check_golden;

/// Bumped whenever the spelling of any value that crosses changes, a draw list's included: host and guest may be built from different sources, and this is the one number that says they spell alike.
pub const WIRE_VERSION: u16 = 1;

/// Bytes that are not the spelling of the value they were read as.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WireError {
    Truncated,
    Tag { of: &'static str, tag: u8 },
    Invalid(&'static str),
    Trailing(usize),
}

impl fmt::Display for WireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated => f.write_str("the bytes end early"),
            Self::Tag { of, tag } => write!(f, "unknown {of} tag {tag}"),
            Self::Invalid(what) => write!(f, "invalid {what}"),
            Self::Trailing(left) => write!(f, "{left} bytes left after the value"),
        }
    }
}

impl std::error::Error for WireError {}

/// A value with a byte spelling: [`put`](Self::put) writes it and [`take`](Self::take) reads it back, refusing bytes `put` would never have written.
pub trait Wire: Sized {
    fn put(&self, w: &mut Writer);
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError>;
}

/// `value` spelled into a buffer of its own.
pub fn encode<T: Wire>(value: &T) -> Vec<u8> {
    let mut w = Writer::new();
    value.put(&mut w);
    w.into_vec()
}

/// The one value `bytes` spell, refusing bytes left over after it.
pub fn decode<T: Wire>(bytes: &[u8]) -> Result<T, WireError> {
    let mut r = Reader::new(bytes);
    let value = T::take(&mut r)?;
    match r.remaining() {
        0 => Ok(value),
        left => Err(WireError::Trailing(left)),
    }
}

/// Spells a struct as its listed fields, in the order listed; every field has to be listed, since reading builds the struct from them.
#[macro_export]
macro_rules! wire_struct {
    ($ty:ty { $($field:ident),+ $(,)? }) => {
        impl $crate::Wire for $ty {
            fn put(&self, w: &mut $crate::Writer) {
                $($crate::Wire::put(&self.$field, w);)+
            }
            fn take(r: &mut $crate::Reader<'_>) -> ::core::result::Result<Self, $crate::WireError> {
                ::core::result::Result::Ok(Self { $($field: $crate::Wire::take(r)?),+ })
            }
        }
    };
}

/// Spells an enum as the tag written beside each variant, then that variant's fields: `Unit = 0`, `Tuple(a, b) = 1`, `Named { a, b } = 2`.
#[macro_export]
macro_rules! wire_enum {
    ($ty:ident { $($variant:ident $(($($field:ident),+ $(,)?))? $({ $($named:ident),+ $(,)? })? = $tag:literal),+ $(,)? }) => {
        impl $crate::Wire for $ty {
            fn put(&self, w: &mut $crate::Writer) {
                match self {
                    $(Self::$variant $(($($field),+))? $({ $($named),+ })? => {
                        w.u8($tag);
                        $($($crate::Wire::put($field, w);)+)?
                        $($($crate::Wire::put($named, w);)+)?
                    })+
                }
            }
            fn take(r: &mut $crate::Reader<'_>) -> ::core::result::Result<Self, $crate::WireError> {
                match r.u8()? {
                    $($tag => ::core::result::Result::Ok(Self::$variant
                        $(($({
                            let $field = $crate::Wire::take(r)?;
                            $field
                        }),+))?
                        $({ $($named: $crate::Wire::take(r)?),+ })?
                    ),)+
                    tag => ::core::result::Result::Err($crate::WireError::Tag { of: stringify!($ty), tag }),
                }
            }
        }
    };
}

#[cfg(test)]
#[path = "lib_test.rs"]
mod tests;
