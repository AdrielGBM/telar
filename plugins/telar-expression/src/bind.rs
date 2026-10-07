//! Reactive binding: a compiled expression and a resolver become a [`Memo`] that follows what the expression reads.

use std::cell::RefCell;

use reactive_core::{Memo, memo};

use crate::{Compiled, Error, Resolver, Value};

/// The current result of `compiled`, recomputed whenever something an evaluation read moves.
///
/// Dependencies are whatever `resolver` read *on the last evaluation*, tracked by the reactive runtime as it happens, so they are exact and dynamic: an `if` subscribes only to the branch it took, `a && b` reads `b` only while `a` holds, and a reference that stops being read stops being a dependency. The memo notifies onward only when the result actually changes.
pub fn bind(compiled: Compiled, resolver: impl Resolver + 'static) -> Memo<Result<Value, Error>> {
    memo(move || compiled.evaluate(&resolver))
}

/// What a property bound with [`bind_held`] shows: the last value that evaluated, and the error of the current evaluation if it failed.
#[derive(Debug, Clone, PartialEq)]
pub struct Held {
    /// The latest value that evaluated, kept through failures after it. `None` until the first success.
    pub value: Option<Value>,
    /// Why the current evaluation failed, for an editor to show beside the expression. `None` when it succeeded.
    pub error: Option<Error>,
}

/// [`bind`], keeping the last good value through an evaluation error, so a property that briefly has no answer — a division by a reading that passed through zero — keeps drawing what it drew instead of falling back to nothing.
pub fn bind_held(compiled: Compiled, resolver: impl Resolver + 'static) -> Memo<Held> {
    let last_good = RefCell::new(None::<Value>);
    memo(move || match compiled.evaluate(&resolver) {
        Ok(value) => {
            last_good.replace(Some(value.clone()));
            Held {
                value: Some(value),
                error: None,
            }
        }
        Err(error) => Held {
            value: last_good.borrow().clone(),
            error: Some(error),
        },
    })
}

#[cfg(test)]
#[path = "bind_test.rs"]
mod tests;
