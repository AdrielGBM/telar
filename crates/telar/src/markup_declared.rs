//! A `.rsx` attribute handed an `Option`: `Some` declares its value, `None` declares nothing, so what the tree around it declared, or the default, stands. `font_weight:(if $current { Some(strong) } else { None })` is a weight only while current.
//!
//! The transpiler cannot see an expression's type, so the value's own type decides, where the generated code is concrete: [`Handed`] wraps the value, and its `declared()` is the `Option` itself for an `Option`, and `Some` of anything else.

/// A value an attribute was handed, asked whether it declares anything.
#[doc(hidden)]
pub struct Handed<T>(pub T);

impl<T: Clone> Handed<Option<T>> {
    /// What an `Option` declares: its value, or nothing.
    pub fn declared(&self) -> Option<T> {
        self.0.clone()
    }
}

/// What any other value declares: itself. A trait, so the `Option` impl above, being inherent, is the one picked for an `Option`.
#[doc(hidden)]
pub trait HandsItself<T> {
    fn declared(&self) -> Option<T>;
}

impl<T: Clone> HandsItself<T> for Handed<T> {
    fn declared(&self) -> Option<T> {
        Some(self.0.clone())
    }
}

/// Applies a declaration only when there is one: `style.declaring_if(value, |style, v| style.with_x(v))`.
#[doc(hidden)]
pub trait DeclaringIf: Sized {
    fn declaring_if<V>(self, value: Option<V>, declare: impl FnOnce(Self, V) -> Self) -> Self {
        match value {
            Some(value) => declare(self, value),
            None => self,
        }
    }
}

impl<S> DeclaringIf for S {}

#[cfg(test)]
#[path = "markup_declared.test.rs"]
mod tests;
