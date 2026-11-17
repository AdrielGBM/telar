//! [`Handler`]: what a press runs, shared by every place that may run it.

use std::rc::Rc;

/// What a press runs: a box's [`on_press`](crate::StyledContainer::on_press), and a component's press prop that is handed on to one.
///
/// Cheap to clone, so a component can keep its prop and still hand it to the box that answers the press, and a view rebuilt from the same props hands the same callback again. Any `Fn() + 'static` converts into one.
#[derive(Clone)]
pub struct Handler(Rc<dyn Fn()>);

impl Handler {
    /// Runs the callback.
    pub fn run(&self) {
        (self.0)()
    }
}

impl<F: Fn() + 'static> From<F> for Handler {
    fn from(run: F) -> Self {
        Self(Rc::new(run))
    }
}

/// A closure already shared, which is how markup hands a closure to a component's prop.
impl<F: Fn() + 'static> From<Rc<F>> for Handler {
    fn from(run: Rc<F>) -> Self {
        Self(run)
    }
}

/// What markup may forward to a box's `on_press:`: a closure, a [`Handler`], or an `Option<Handler>` a caller may have left unset — all handed to [`maybe_on_press`](crate::StyledContainer::maybe_on_press).
///
/// Decided by the value's type, so a view forwards a binding without saying which of the three it holds.
pub trait IntoPress {
    /// The handler this value holds, `None` for a caller that gave none.
    fn into_press(self) -> Option<Handler>;
}

impl<F: Fn() + 'static> IntoPress for F {
    fn into_press(self) -> Option<Handler> {
        Some(Handler::from(self))
    }
}

impl IntoPress for Handler {
    fn into_press(self) -> Option<Handler> {
        Some(self)
    }
}

impl IntoPress for Option<Handler> {
    fn into_press(self) -> Option<Handler> {
        self
    }
}

#[cfg(test)]
#[path = "handler.test.rs"]
mod tests;
