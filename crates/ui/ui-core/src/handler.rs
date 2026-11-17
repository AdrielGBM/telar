//! [`Handler`]: a callback shared by every place that may run it.

use std::rc::Rc;

/// A callback, named by the function type it is called as: `Handler` (`Handler<fn()>`) takes no argument, `Handler<fn(bool)>` one, `Handler<fn(f32, f32)>` two.
///
/// What every callback in the framework is: a box's [`on_press`](crate::StyledContainer::on_press), a component's `on_toggle` or `on_select` prop, an overlay's dismiss. Cheap to clone, so a component can keep its prop and still hand it to the box that answers the event, and a view rebuilt from the same props hands the same callback again.
///
/// Any closure called as `S` converts into one: `|| …` into a `Handler`, `|on| …` into a `Handler<fn(bool)>`. An argument is owned, never borrowed: `Handler<fn(&str)>` has no [`HandlerSignature`], so a callback about a borrowed value takes it owned, a `String` for a `&str`.
///
/// A component's callback prop holds one: its setter takes the closure itself, whose argument types are read from `S`, and its `maybe_` setter a handler held already, as an `Option`.
pub struct Handler<S: HandlerSignature = fn()>(Rc<S::Callback>);

/// The function types a [`Handler`] is named by: `fn()`, and `fn(A)` to `fn(A, B, C)` for owned arguments.
pub trait HandlerSignature: sealed::Sealed {
    /// The closure a handler of this signature holds.
    type Callback: ?Sized;
}

mod sealed {
    pub trait Sealed {}
}

macro_rules! signatures {
    ($(($($arg:ident: $ty:ident),*)),* $(,)?) => {
        $(
            impl<$($ty),*> sealed::Sealed for fn($($ty),*) {}

            impl<$($ty),*> HandlerSignature for fn($($ty),*) {
                type Callback = dyn Fn($($ty),*);
            }

            impl<$($ty,)* F: Fn($($ty),*) + 'static> From<F> for Handler<fn($($ty),*)> {
                fn from(run: F) -> Self {
                    Self(Rc::new(run))
                }
            }

            impl<$($ty),*> Handler<fn($($ty),*)> {
                /// Runs the callback.
                pub fn run(&self, $($arg: $ty),*) {
                    (self.0)($($arg),*)
                }
            }

            impl<$($ty,)* F: Fn($($ty),*) + 'static> IntoHandler<fn($($ty),*)> for F {
                fn into_handler(self) -> Option<Handler<fn($($ty),*)>> {
                    Some(Handler::from(self))
                }
            }
        )*
    };
}

signatures!((), (a: A), (a: A, b: B), (a: A, b: B, c: C));

impl<S: HandlerSignature> Clone for Handler<S> {
    fn clone(&self) -> Self {
        Self(Rc::clone(&self.0))
    }
}

/// What markup may forward to a box's event, `on_press:` or `on_hover:`, or to a component's callback prop: a closure, a [`Handler`], or an `Option<Handler>` a caller may have left unset — all handed to the event's or the prop's `maybe_` form, such as [`maybe_on_press`](crate::StyledContainer::maybe_on_press).
///
/// Decided by the value's type, so a view forwards a binding without saying which of the three it holds.
pub trait IntoHandler<S: HandlerSignature> {
    /// The handler this value holds, `None` for a caller that gave none.
    fn into_handler(self) -> Option<Handler<S>>;
}

impl<S: HandlerSignature> IntoHandler<S> for Handler<S> {
    fn into_handler(self) -> Option<Handler<S>> {
        Some(self)
    }
}

impl<S: HandlerSignature> IntoHandler<S> for Option<Handler<S>> {
    fn into_handler(self) -> Option<Handler<S>> {
        self
    }
}

#[cfg(test)]
#[path = "handler.test.rs"]
mod tests;
