//! [`Type`]: what an expression produces, known before it runs; [`Signature`]: what a function takes and gives back.

use std::fmt;

/// The type of a value, which the checker settles for every expression before it is ever evaluated.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
    Number,
    Text,
    Bool,
    Color,
    List(Box<Type>),
    /// The type of something that never yields a value: the element of an empty list, or a part of an expression that already failed to check.
    ///
    /// It fits wherever any type is expected, which is what lets `{}` be a list of anything and keeps one mistake from being reported again by everything around it.
    Never,
}

impl Type {
    pub fn list(element: Type) -> Self {
        Self::List(Box::new(element))
    }

    /// The one type both sides can be, if there is one: `{}` and `{1}` are both lists of numbers.
    pub fn unify(&self, other: &Type) -> Option<Type> {
        match (self, other) {
            (Type::Never, other) | (other, Type::Never) => Some(other.clone()),
            (Type::List(a), Type::List(b)) => a.unify(b).map(Type::list),
            (a, b) if a == b => Some(a.clone()),
            _ => None,
        }
    }

    /// Whether a value of type `other` can stand where this type is expected.
    pub fn accepts(&self, other: &Type) -> bool {
        self.unify(other).as_ref() == Some(self)
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Number => f.write_str("number"),
            Type::Text => f.write_str("text"),
            Type::Bool => f.write_str("bool"),
            Type::Color => f.write_str("colour"),
            Type::List(element) if **element == Type::Never => f.write_str("empty list"),
            Type::List(element) => write!(f, "list of {element}"),
            Type::Never => f.write_str("nothing"),
        }
    }
}

/// One parameter, or the result, of a [`Signature`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Pattern {
    Exact(Type),
    /// Any type at all. Not allowed as a result, which must be known.
    Any,
    /// A type variable: every place one number appears must be the same type, and the result may name it. `at` is `(list of 0, number) -> 0`.
    Var(u8),
    /// A list whose element matches the inner pattern.
    List(Box<Pattern>),
}

impl Pattern {
    pub fn list(element: Pattern) -> Self {
        Self::List(Box::new(element))
    }

    fn bind(&self, ty: &Type, vars: &mut Vec<Option<Type>>) -> bool {
        match self {
            Pattern::Any => true,
            Pattern::Exact(expected) => expected.accepts(ty),
            Pattern::Var(index) => {
                let index = usize::from(*index);
                if vars.len() <= index {
                    vars.resize(index + 1, None);
                }
                match &vars[index] {
                    None => {
                        vars[index] = Some(ty.clone());
                        true
                    }
                    Some(bound) => match bound.unify(ty) {
                        Some(unified) => {
                            vars[index] = Some(unified);
                            true
                        }
                        None => false,
                    },
                }
            }
            Pattern::List(element) => match ty {
                Type::List(inner) => element.bind(inner, vars),
                Type::Never => true,
                _ => false,
            },
        }
    }

    fn resolve(&self, vars: &[Option<Type>]) -> Type {
        match self {
            Pattern::Exact(ty) => ty.clone(),
            Pattern::Any => Type::Never,
            Pattern::Var(index) => vars
                .get(usize::from(*index))
                .cloned()
                .flatten()
                .unwrap_or(Type::Never),
            Pattern::List(element) => Type::list(element.resolve(vars)),
        }
    }

    fn mentions_any(&self) -> bool {
        match self {
            Pattern::Any => true,
            Pattern::List(element) => element.mentions_any(),
            Pattern::Exact(_) | Pattern::Var(_) => false,
        }
    }
}

impl From<Type> for Pattern {
    fn from(ty: Type) -> Self {
        Self::Exact(ty)
    }
}

impl fmt::Display for Pattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Pattern::Exact(ty) => ty.fmt(f),
            Pattern::Any => f.write_str("any"),
            Pattern::Var(index) => write!(f, "{}", char::from(b'T'.saturating_add(*index))),
            Pattern::List(element) => write!(f, "list of {element}"),
        }
    }
}

/// What a function takes and what it gives back, checked against the argument types at compile time.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Signature {
    pub parameters: Vec<Pattern>,
    /// What any further arguments must match, when the function takes a variable number of them.
    pub rest: Option<Pattern>,
    pub returns: Pattern,
}

/// Why a call's arguments fit no [`Signature`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Mismatch {
    Count,
    Argument(usize),
}

impl Signature {
    /// A signature taking exactly `parameters`.
    ///
    /// # Panics
    ///
    /// When `returns` contains [`Pattern::Any`]: a function's result type must be known, and this is a mistake in the code registering it, never in an expression.
    pub fn new(parameters: impl IntoIterator<Item = Pattern>, returns: impl Into<Pattern>) -> Self {
        let returns = returns.into();
        assert!(
            !returns.mentions_any(),
            "a function's result type must be known, not `any`"
        );
        Self {
            parameters: parameters.into_iter().collect(),
            rest: None,
            returns,
        }
    }

    /// The same signature, also taking any number of further arguments matching `pattern`.
    pub fn rest(mut self, pattern: impl Into<Pattern>) -> Self {
        self.rest = Some(pattern.into());
        self
    }

    pub(crate) fn apply(&self, arguments: &[Type]) -> Result<Type, Mismatch> {
        let fixed = self.parameters.len();
        if arguments.len() < fixed || (self.rest.is_none() && arguments.len() > fixed) {
            return Err(Mismatch::Count);
        }
        let mut vars = Vec::new();
        for (index, argument) in arguments.iter().enumerate() {
            let pattern = self.parameters.get(index).or(self.rest.as_ref());
            if !pattern.is_some_and(|pattern| pattern.bind(argument, &mut vars)) {
                return Err(Mismatch::Argument(index));
            }
        }
        Ok(self.returns.resolve(&vars))
    }

    /// The pattern argument `index` must match.
    pub fn parameter(&self, index: usize) -> Option<&Pattern> {
        self.parameters.get(index).or(self.rest.as_ref())
    }

    /// How many arguments a call must pass, and whether that is a minimum.
    pub(crate) fn arity(&self) -> (usize, bool) {
        (self.parameters.len(), self.rest.is_some())
    }
}

impl fmt::Display for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("(")?;
        for (index, parameter) in self.parameters.iter().enumerate() {
            if index > 0 {
                f.write_str(", ")?;
            }
            parameter.fmt(f)?;
        }
        if let Some(rest) = &self.rest {
            if !self.parameters.is_empty() {
                f.write_str(", ")?;
            }
            write!(f, "…{rest}")?;
        }
        write!(f, ") -> {}", self.returns)
    }
}
