//! [`PreviewArg`]: the types a control can edit, and how each one crosses to and from an [`ArgValue`].

use std::cell::RefCell;
use std::collections::HashSet;
use std::mem::ManuallyDrop;

use reactive_core::{Reactive, RwSignal, signal};

use super::{ArgValue, ControlKind};
use crate::Color;

/// A type an arg can hold and a control can edit.
///
/// Implemented for `bool`, the integer and float primitives, `&'static str`, `String`, [`Color`], `Option<T>`, [`Reactive<T>`] and [`RwSignal<T>`]. An enum gets it from `#[derive(PreviewArg)]`, which lists its variants for a choice control.
pub trait PreviewArg: Sized + 'static {
    const CONTROL: ControlKind;

    fn to_arg_value(&self) -> ArgValue;

    /// `None` when `value` is not one this type can hold, which the caller answers by keeping what it had.
    fn from_arg_value(value: &ArgValue) -> Option<Self>;

    /// Whether [`Self::from_arg_value`] would succeed, without building the value.
    fn accepts(value: &ArgValue) -> bool {
        Self::from_arg_value(value).is_some()
    }
}

impl PreviewArg for bool {
    const CONTROL: ControlKind = ControlKind::Toggle;

    fn to_arg_value(&self) -> ArgValue {
        ArgValue::Bool(*self)
    }

    fn from_arg_value(value: &ArgValue) -> Option<Self> {
        match value {
            ArgValue::Bool(value) => Some(*value),
            _ => None,
        }
    }
}

macro_rules! integer_args {
    ($($ty:ty),*) => {$(
        impl PreviewArg for $ty {
            const CONTROL: ControlKind = ControlKind::INTEGER;

            fn to_arg_value(&self) -> ArgValue {
                ArgValue::Int(*self as i128)
            }

            fn from_arg_value(value: &ArgValue) -> Option<Self> {
                match *value {
                    ArgValue::Int(value) => <$ty>::try_from(value).ok(),
                    ArgValue::Float(value) if value.fract() == 0.0 => {
                        <$ty>::try_from(value as i128).ok()
                    }
                    _ => None,
                }
            }
        }
    )*};
}

// `as`, since `isize` and `usize` have no `From` into `i128`; every type listed fits in one.
integer_args!(i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, usize);

impl PreviewArg for f64 {
    const CONTROL: ControlKind = ControlKind::FLOAT;

    fn to_arg_value(&self) -> ArgValue {
        ArgValue::Float(*self)
    }

    fn from_arg_value(value: &ArgValue) -> Option<Self> {
        match *value {
            ArgValue::Float(value) => Some(value),
            ArgValue::Int(value) => Some(value as f64),
            _ => None,
        }
    }
}

impl PreviewArg for f32 {
    const CONTROL: ControlKind = ControlKind::FLOAT;

    fn to_arg_value(&self) -> ArgValue {
        ArgValue::Float(f64::from(*self))
    }

    fn from_arg_value(value: &ArgValue) -> Option<Self> {
        f64::from_arg_value(value).map(|value| value as f32)
    }
}

impl PreviewArg for String {
    const CONTROL: ControlKind = ControlKind::TEXT;

    fn to_arg_value(&self) -> ArgValue {
        ArgValue::Text(self.clone())
    }

    fn from_arg_value(value: &ArgValue) -> Option<Self> {
        match value {
            ArgValue::Text(text) => Some(text.clone()),
            _ => None,
        }
    }
}

/// An override is leaked to become `'static`, which only a preview build ever does. Each distinct text leaks once, however often a control sends it back.
impl PreviewArg for &'static str {
    const CONTROL: ControlKind = ControlKind::TEXT;

    fn to_arg_value(&self) -> ArgValue {
        ArgValue::Text((*self).to_string())
    }

    fn from_arg_value(value: &ArgValue) -> Option<Self> {
        match value {
            ArgValue::Text(text) => Some(leaked(text)),
            _ => None,
        }
    }

    fn accepts(value: &ArgValue) -> bool {
        matches!(value, ArgValue::Text(_))
    }
}

// `ManuallyDrop`, as in `hot_state`: a preview build is often a dylib, and a TLS destructor registered from one makes unloading it unsafe.
thread_local! {
    static LEAKED: ManuallyDrop<RefCell<HashSet<&'static str>>> =
        ManuallyDrop::new(RefCell::new(HashSet::new()));
}

fn leaked(text: &str) -> &'static str {
    LEAKED.with(|leaked| {
        let mut leaked = leaked.borrow_mut();
        if let Some(held) = leaked.get(text) {
            return *held;
        }
        let held: &'static str = Box::leak(text.to_owned().into_boxed_str());
        leaked.insert(held);
        held
    })
}

impl PreviewArg for Color {
    const CONTROL: ControlKind = ControlKind::Color;

    fn to_arg_value(&self) -> ArgValue {
        ArgValue::Color(*self)
    }

    fn from_arg_value(value: &ArgValue) -> Option<Self> {
        match value {
            ArgValue::Color(color) => Some(*color),
            _ => None,
        }
    }
}

impl<T: PreviewArg> PreviewArg for Option<T> {
    const CONTROL: ControlKind = ControlKind::Optional(&T::CONTROL);

    fn to_arg_value(&self) -> ArgValue {
        match self {
            Some(value) => value.to_arg_value(),
            None => ArgValue::Unset,
        }
    }

    fn from_arg_value(value: &ArgValue) -> Option<Self> {
        match value {
            ArgValue::Unset => Some(None),
            value => T::from_arg_value(value).map(Some),
        }
    }

    fn accepts(value: &ArgValue) -> bool {
        matches!(value, ArgValue::Unset) || T::accepts(value)
    }
}

/// An override is held as a constant: the control is what changes it, not the state the default followed.
impl<T: PreviewArg + Clone> PreviewArg for Reactive<T> {
    const CONTROL: ControlKind = T::CONTROL;

    fn to_arg_value(&self) -> ArgValue {
        self.get().to_arg_value()
    }

    fn from_arg_value(value: &ArgValue) -> Option<Self> {
        T::from_arg_value(value).map(Reactive::Const)
    }

    fn accepts(value: &ArgValue) -> bool {
        T::accepts(value)
    }
}

/// An override is a fresh signal, owned by whatever is being built when the arg is read.
impl<T: PreviewArg> PreviewArg for RwSignal<T> {
    const CONTROL: ControlKind = T::CONTROL;

    fn to_arg_value(&self) -> ArgValue {
        self.peek_with(T::to_arg_value)
    }

    fn from_arg_value(value: &ArgValue) -> Option<Self> {
        T::from_arg_value(value).map(signal)
    }

    fn accepts(value: &ArgValue) -> bool {
        T::accepts(value)
    }
}

#[cfg(test)]
#[path = "arg_test.rs"]
mod tests;
