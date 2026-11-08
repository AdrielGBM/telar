//! The byte spelling of every value that crosses between a Telar host and a guest library.
//!
//! A guest links its own copy of std and its own allocator, so nothing it owns may be handed to the host to keep or free, nor the other way round. What crosses is bytes instead: one side spells a value into a buffer it owns and lends it, and the other reads it back into values it allocated itself. Rich values travel this way, as display lists commonly do across a process boundary, and only the handful of `repr(C)` building blocks in `platform-core::guest` cross as themselves.
//!
//! Each type's spelling lives beside the type, in the crate that defines it, through [`Wire`]; this crate holds the primitives they are spelled in and the spelling of the standard types they are built from. Every enum names its tags out loud, so the bytes never follow declaration order and a reordered variant changes nothing.

#![forbid(unsafe_code)]
#![warn(rustdoc::broken_intra_doc_links)]

use std::fmt;

mod bytes;
#[cfg(any(test, feature = "golden"))]
mod golden;
mod shape;
mod value;

pub use bytes::{Reader, Writer};
#[cfg(any(test, feature = "golden"))]
pub use golden::check_golden;
#[doc(hidden)]
pub use shape::{FieldShapes, Recursive, fold, fold_shape, shape_at, shape_named, shapes_at};

/// Bumped whenever the rules of the spelling itself change, which no type's [`Shaped::SHAPE`] shows: how a value, a tag or a draw list's interned payloads are laid into bytes. What a type spells is its shape's to say, which the contract stamp folds in.
pub const WIRE_VERSION: u16 = 4;

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

/// Bytes whose spelling has a shape: every [`Wire`] value, and what crosses spelled some other way, such as a draw list.
pub trait Shaped {
    /// What this type's spelling is made of: built from its name and, for a type the macros spell, every variant's name and tag and every field's name and shape, so a host and a guest that would read each other's bytes differently differ here first.
    const SHAPE: u64;
}

/// A value with a byte spelling: [`put`](Self::put) writes it and [`take`](Self::take) reads it back, refusing bytes `put` would never have written.
pub trait Wire: Shaped + Sized {
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

/// `hash` with each listed field of `$ty` folded in, its name and its shape, in the order listed: what a spelling built from those fields is made of.
#[doc(hidden)]
#[macro_export]
macro_rules! fold_fields {
    ($hash:expr, $ty:ty { $($field:ident),+ $(,)? }) => {{
        let hash = $hash;
        $(let hash = $crate::fold_shape(
            $crate::fold(hash, stringify!($field).as_bytes()),
            $crate::shape_at(|value: &$ty| &raw const value.$field),
        );)+
        hash
    }};
}

/// Spells a struct as its listed fields, in the order listed; every field has to be listed, since reading builds the struct from them.
#[macro_export]
macro_rules! wire_struct {
    ($ty:ty { $($field:ident),+ $(,)? }) => {
        impl $crate::Shaped for $ty {
            const SHAPE: u64 =
                $crate::fold_fields!($crate::shape_named(stringify!($ty)), $ty { $($field),+ });
        }
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

/// Spells an enum as the tag written beside each variant, then that variant's fields: `Unit = 0`, `Tuple(a, b) = 1`, `Named { a, b } = 2`. A field written `field: Self` holds the enum again, behind a box or a sequence: its shape is [`Recursive`]'s, since the enum's own would be defined by itself.
#[macro_export]
macro_rules! wire_enum {
    (@field $field:ident) => { $field as *const _ };
    (@field $field:ident Self) => {{
        let _ = $field;
        ::core::ptr::null::<$crate::Recursive>()
    }};
    ($ty:ident { $($variant:ident $(($($field:ident $(: $field_recursive:ident)?),+ $(,)?))? $({ $($named:ident $(: $recursive:ident)?),+ $(,)? })? = $tag:literal),+ $(,)? }) => {
        impl $crate::Shaped for $ty {
            #[allow(unreachable_patterns)]
            const SHAPE: u64 = {
                let hash = $crate::shape_named(stringify!($ty));
                $(
                    let hash = $crate::fold($crate::fold(hash, stringify!($variant).as_bytes()), &[$tag]);
                    $(let hash = $crate::fold_shape(hash, $crate::shapes_at(|value: &$ty| match value {
                        $ty::$variant($($field),+) => ($($crate::wire_enum!(@field $field $($field_recursive)?),)+),
                        _ => unreachable!(),
                    }));)?
                    $(let hash = $crate::fold_shape(hash, $crate::shapes_at(|value: &$ty| match value {
                        $ty::$variant { $($named),+ } => ($($crate::wire_enum!(@field $named $($recursive)?),)+),
                        _ => unreachable!(),
                    }));)?
                )+
                hash
            };
        }
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
#[path = "lib.test.rs"]
mod tests;
