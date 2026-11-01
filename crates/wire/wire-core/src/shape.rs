//! What a type's spelling is made of, as one number a host and a guest compare before either reads the other's bytes.

use crate::Shaped;

const SEED: u64 = 0xcbf2_9ce4_8422_2325;
const PRIME: u64 = 0x0000_0100_0000_01b3;

/// `hash` with `bytes` folded in.
pub const fn fold(mut hash: u64, bytes: &[u8]) -> u64 {
    let mut at = 0;
    while at < bytes.len() {
        hash ^= bytes[at] as u64;
        hash = hash.wrapping_mul(PRIME);
        at += 1;
    }
    hash
}

/// `hash` with another shape folded in.
pub const fn fold_shape(hash: u64, shape: u64) -> u64 {
    fold(hash, &shape.to_le_bytes())
}

/// The shape of a spelling known by its name alone.
pub const fn shape_named(name: &str) -> u64 {
    fold(SEED, name.as_bytes())
}

/// The shape of the value `field` points at, which only its type decides: `field` is never called.
pub const fn shape_at<S, T: Shaped>(field: fn(&S) -> *const T) -> u64 {
    let _ = field;
    T::SHAPE
}

/// The fields of one enum variant, as the pointers a projection of the variant yields.
pub trait FieldShapes {
    const SHAPE: u64;
}

macro_rules! field_shapes {
    ($($field:ident),+) => {
        impl<$($field: Shaped),+> FieldShapes for ($(*const $field,)+) {
            const SHAPE: u64 = {
                let hash = SEED;
                $(let hash = fold_shape(hash, $field::SHAPE);)+
                hash
            };
        }
    };
}

field_shapes!(A);
field_shapes!(A, B);
field_shapes!(A, B, C);
field_shapes!(A, B, C, D);
field_shapes!(A, B, C, D, E);
field_shapes!(A, B, C, D, E, F);
field_shapes!(A, B, C, D, E, F, G);
field_shapes!(A, B, C, D, E, F, G, H);
field_shapes!(A, B, C, D, E, F, G, H, I);
field_shapes!(A, B, C, D, E, F, G, H, I, J);
field_shapes!(A, B, C, D, E, F, G, H, I, J, K);
field_shapes!(A, B, C, D, E, F, G, H, I, J, K, L);

/// The shape of the fields a variant projection yields, which only their types decide: `variant` is never called.
pub const fn shapes_at<S, T: FieldShapes>(variant: fn(&S) -> T) -> u64 {
    let _ = variant;
    T::SHAPE
}

/// Where a type holds itself again, which its shape cannot fold in without being defined by itself: the recursion is folded as this.
pub enum Recursive {}

impl Shaped for Recursive {
    const SHAPE: u64 = shape_named("recursive");
}
