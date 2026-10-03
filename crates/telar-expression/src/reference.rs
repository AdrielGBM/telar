//! [`Reference`]: a `$name.path` in an expression, and the two traits a host answers them through — [`Environment`] for their types when compiling, [`Resolver`] for their values when evaluating.

use std::fmt;

use crate::{HostError, Type, Value};

/// A `$name` or `$name.path.to.field` an expression reads. What it names is entirely the host's business.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Reference {
    pub name: String,
    pub path: Vec<String>,
}

impl Reference {
    pub fn new(name: impl Into<String>, path: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            name: name.into(),
            path: path.into_iter().map(Into::into).collect(),
        }
    }

    /// The reference without its `$`: `battery.level`.
    pub fn dotted(&self) -> String {
        let mut dotted = self.name.clone();
        for segment in &self.path {
            dotted.push('.');
            dotted.push_str(segment);
        }
        dotted
    }
}

impl fmt::Display for Reference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "${}", self.dotted())
    }
}

/// What the host declares to the type checker: the type of every reference an expression may read.
///
/// A closure `Fn(&Reference) -> Result<Type, HostError>` is one. The `Err` is shown at the reference as [`crate::ErrorCode::Host`], so a host can say more than "unknown" — that a source has no such field, say, and which ones it has — and translate it from the key and arguments of its [`HostError`].
pub trait Environment {
    fn reference(&self, reference: &Reference) -> Result<Type, HostError>;
}

impl<F: Fn(&Reference) -> Result<Type, HostError>> Environment for F {
    fn reference(&self, reference: &Reference) -> Result<Type, HostError> {
        self(reference)
    }
}

/// How the evaluator reads a reference's current value.
///
/// For a reactive binding, read the host's signal with `.get()` here: only what an evaluation actually reads becomes a dependency, so an `if` branch not taken subscribes to nothing. A value that does not fit the type the [`Environment`] declared is an evaluation error, never trusted.
///
/// A closure `Fn(&Reference) -> Result<Value, HostError>` is one; the `Err` is shown at the reference, for a reading that has no value yet.
pub trait Resolver {
    fn read(&self, reference: &Reference) -> Result<Value, HostError>;
}

impl<F: Fn(&Reference) -> Result<Value, HostError>> Resolver for F {
    fn read(&self, reference: &Reference) -> Result<Value, HostError> {
        self(reference)
    }
}

/// An environment with no references at all, for a closed expression such as a calculator's. Every reference is an error.
#[derive(Debug, Clone, Copy, Default)]
pub struct Closed;

impl Environment for Closed {
    fn reference(&self, reference: &Reference) -> Result<Type, HostError> {
        Err(HostError::new("unreadable", format!("{reference} cannot be read here")).arg(reference))
    }
}

impl Resolver for Closed {
    fn read(&self, reference: &Reference) -> Result<Value, HostError> {
        Err(HostError::new("unreadable", format!("{reference} cannot be read here")).arg(reference))
    }
}
