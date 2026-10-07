//! [`Registry`]: the functions and constants an expression may name, which a host extends with families of its own.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use geometry_core::Color;

use crate::{ErrorCode, Signature, Value, ValueKind, families, is_identifier};

/// What a function's implementation hands back when it has no answer for the arguments it was given.
///
/// A host's own function reports its failure as [`ErrorCode::Host`] with a [`crate::HostError`]; the crate's accessors report the others.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallError {
    /// The argument at fault, counted from zero, so the error points at it rather than at the whole call.
    pub argument: Option<usize>,
    pub code: ErrorCode,
}

impl CallError {
    pub fn new(code: ErrorCode) -> Self {
        Self {
            argument: None,
            code,
        }
    }

    pub fn argument(index: usize, code: ErrorCode) -> Self {
        Self {
            argument: Some(index),
            code,
        }
    }
}

/// The evaluated arguments of a call, with typed accessors.
///
/// The checker has already matched them against the [`Signature`], so a wrong type here would be a host's function lying about its own signature; the accessors still answer it with a [`CallError`] rather than a panic.
#[derive(Debug, Clone, Copy)]
pub struct Arguments<'a>(pub &'a [Value]);

impl<'a> Arguments<'a> {
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn value(&self, index: usize) -> Result<&'a Value, CallError> {
        self.0
            .get(index)
            .ok_or_else(|| CallError::new(ErrorCode::MissingArgument { index }))
    }

    fn typed<T>(
        &self,
        index: usize,
        expected: ValueKind,
        read: impl FnOnce(&'a Value) -> Option<T>,
    ) -> Result<T, CallError> {
        read(self.value(index)?)
            .ok_or_else(|| CallError::argument(index, ErrorCode::ExpectedArgument { expected }))
    }

    pub fn number(&self, index: usize) -> Result<f64, CallError> {
        self.typed(index, ValueKind::Number, Value::as_number)
    }

    pub fn text(&self, index: usize) -> Result<&'a str, CallError> {
        self.typed(index, ValueKind::Text, Value::as_text)
    }

    pub fn bool(&self, index: usize) -> Result<bool, CallError> {
        self.typed(index, ValueKind::Bool, Value::as_bool)
    }

    pub fn color(&self, index: usize) -> Result<Color, CallError> {
        self.typed(index, ValueKind::Color, Value::as_color)
    }

    pub fn list(&self, index: usize) -> Result<&'a [Value], CallError> {
        self.typed(index, ValueKind::List, Value::as_list)
    }

    /// The arguments from `index` on: a variadic function's tail.
    pub fn rest(&self, index: usize) -> &'a [Value] {
        self.0.get(index..).unwrap_or_default()
    }
}

/// The body of a function: pure, and `Send + Sync` so a compiled expression can move to whichever thread checks or evaluates it.
pub type Implementation = dyn Fn(Arguments<'_>) -> Result<Value, CallError> + Send + Sync;

/// One form of a function: its name, its signature, a line saying what it does, and its body.
#[derive(Clone)]
pub struct Function {
    name: String,
    signature: Signature,
    summary: String,
    implementation: Arc<Implementation>,
}

impl Function {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn signature(&self) -> &Signature {
        &self.signature
    }

    /// One line on what the function does, for an editor's autocomplete.
    pub fn summary(&self) -> &str {
        &self.summary
    }

    pub(crate) fn implementation(&self) -> &Arc<Implementation> {
        &self.implementation
    }
}

impl fmt::Debug for Function {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.name, self.signature)
    }
}

/// A named value: `pi`, `e`.
#[derive(Debug, Clone, PartialEq)]
pub struct Constant {
    pub name: String,
    pub value: Value,
    pub summary: String,
}

/// The functions and constants an expression may call or name.
///
/// Names are matched ignoring ASCII case, so `SQRT(9)` is `sqrt(9)`. A name may carry several forms (overloads): `len` takes a text or a list. The checker picks the most recently registered form whose signature fits, so a host family registered after the standard ones can replace a form of a standard function.
#[derive(Clone, Default)]
pub struct Registry {
    functions: BTreeMap<String, Vec<Function>>,
    constants: BTreeMap<String, Constant>,
}

impl Registry {
    /// A registry with nothing in it.
    pub fn new() -> Self {
        Self::default()
    }

    /// Every family this crate ships: [`families::text`], [`families::math`], [`families::list`] and [`families::color`].
    pub fn standard() -> Self {
        Self::new()
            .with(families::text)
            .with(families::math)
            .with(families::list)
            .with(families::color)
    }

    /// This registry with `family` added: a family is any function registering its functions and constants, the host's own included.
    pub fn with(mut self, family: impl FnOnce(&mut Registry)) -> Self {
        family(&mut self);
        self
    }

    /// Adds one form of `name`.
    ///
    /// # Panics
    ///
    /// When `name` is not a plain identifier, or is one of the words the language reserves (`if`, `true`, `false`): a mistake in the code registering it, never in an expression.
    pub fn function(
        &mut self,
        name: &str,
        signature: Signature,
        summary: &str,
        implementation: impl Fn(Arguments<'_>) -> Result<Value, CallError> + Send + Sync + 'static,
    ) -> &mut Self {
        let name = Self::key(name);
        self.functions
            .entry(name.clone())
            .or_default()
            .push(Function {
                name,
                signature,
                summary: summary.to_string(),
                implementation: Arc::new(implementation),
            });
        self
    }

    /// Adds a constant, replacing one of the same name. The same panics as [`Registry::function`].
    pub fn constant(&mut self, name: &str, value: Value, summary: &str) -> &mut Self {
        let name = Self::key(name);
        self.constants.insert(
            name.clone(),
            Constant {
                name,
                value,
                summary: summary.to_string(),
            },
        );
        self
    }

    /// Every form of every function, by name.
    pub fn functions(&self) -> impl Iterator<Item = &Function> {
        self.functions.values().flatten()
    }

    pub fn constants(&self) -> impl Iterator<Item = &Constant> {
        self.constants.values()
    }

    pub(crate) fn forms(&self, name: &str) -> &[Function] {
        self.functions
            .get(&name.to_ascii_lowercase())
            .map_or(&[], Vec::as_slice)
    }

    pub(crate) fn lookup_constant(&self, name: &str) -> Option<&Constant> {
        self.constants.get(&name.to_ascii_lowercase())
    }

    fn key(name: &str) -> String {
        let key = name.to_ascii_lowercase();
        assert!(
            is_identifier(name) && !matches!(key.as_str(), "if" | "true" | "false"),
            "`{name}` cannot be named in an expression"
        );
        key
    }
}

impl fmt::Debug for Registry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Registry")
            .field("functions", &self.functions.keys().collect::<Vec<_>>())
            .field("constants", &self.constants.keys().collect::<Vec<_>>())
            .finish()
    }
}
