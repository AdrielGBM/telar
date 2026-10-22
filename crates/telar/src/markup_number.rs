//! The numbers a `.rsx` call writes on a component's props, before the prop says which type it wants. Markup writes `level:3` and `width:300` alike, and the transpiler cannot see the props type of a component in another file or crate. So a literal is handed over as an [`Int`] or a [`Float`], and a numeric prop's setter takes its own type or either of these through [`PropNumber`]: the prop decides, the way a Rust literal's type is decided by where it is used.

use crate::Reactive;

/// A whole number as markup wrote it: `3`, `-2`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Int(pub i64);

/// A number with a fraction as markup wrote it: `1.5`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Float(pub f64);

/// What a numeric prop of type `T` takes: a `T`, or a number markup wrote. A setter generic over it still types a Rust literal exactly, since the only integer or float type among its implementors is `T` itself.
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a value of a `{T}` prop",
    note = "a whole-number prop takes a whole number: write `3`, not `3.5`"
)]
pub trait PropNumber<T> {
    fn into_prop(self) -> T;
}

macro_rules! integer_props {
    ($($ty:ty),*) => {$(
        impl PropNumber<$ty> for $ty {
            fn into_prop(self) -> $ty {
                self
            }
        }

        impl PropNumber<$ty> for Int {
            fn into_prop(self) -> $ty {
                <$ty>::try_from(self.0).unwrap_or_else(|_| {
                    panic!("`{}` is out of range for a `{}` prop", self.0, stringify!($ty))
                })
            }
        }

        impl From<Int> for $ty {
            fn from(number: Int) -> Self {
                number.into_prop()
            }
        }

        impl From<Int> for Reactive<$ty> {
            fn from(number: Int) -> Self {
                Reactive::from(<$ty>::from(number))
            }
        }
    )*};
}

macro_rules! float_props {
    ($($ty:ty),*) => {$(
        impl PropNumber<$ty> for $ty {
            fn into_prop(self) -> $ty {
                self
            }
        }

        impl PropNumber<$ty> for Int {
            fn into_prop(self) -> $ty {
                self.0 as $ty
            }
        }

        impl PropNumber<$ty> for Float {
            fn into_prop(self) -> $ty {
                self.0 as $ty
            }
        }

        impl From<Int> for $ty {
            fn from(number: Int) -> Self {
                number.into_prop()
            }
        }

        impl From<Float> for $ty {
            fn from(number: Float) -> Self {
                number.into_prop()
            }
        }

        impl From<Int> for Reactive<$ty> {
            fn from(number: Int) -> Self {
                Reactive::from(<$ty>::from(number))
            }
        }

        impl From<Float> for Reactive<$ty> {
            fn from(number: Float) -> Self {
                Reactive::from(<$ty>::from(number))
            }
        }
    )*};
}

integer_props!(i8, i16, i32, i64, isize, u8, u16, u32, u64, usize);
float_props!(f32, f64);

#[cfg(test)]
#[path = "markup_number_test.rs"]
mod tests;
