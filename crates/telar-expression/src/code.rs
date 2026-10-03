//! [`ErrorCode`]: what went wrong, as a typed value with its arguments, so a host can render it in its own language.

use std::fmt;

use crate::{Pattern, Reference, Signature, Type};

/// A failure a host's own code reports: from [`crate::Environment::reference`], [`crate::Resolver::read`] or a family's function.
///
/// The crate cannot know what a host's sources or functions may fail with, so the host describes it: a `key` naming the entry in its own message catalogue, the `args` that entry interpolates, and the English `message` that [`fmt::Display`] and [`crate::render`] print. A host that does not localize passes only the message; one that does maps `key` and `args` to its catalogue and ignores `message`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostError {
    pub key: String,
    pub args: Vec<String>,
    pub message: String,
}

impl HostError {
    /// An error with no arguments: the key of the catalogue entry and its English wording.
    pub fn new(key: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            args: Vec::new(),
            message: message.into(),
        }
    }

    /// This error with the next argument its catalogue entry interpolates.
    pub fn arg(mut self, arg: impl fmt::Display) -> Self {
        self.args.push(arg.to_string());
        self
    }
}

impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

/// What the parser found where it wanted something else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Found {
    /// A number literal.
    Number,
    /// A text literal.
    Text,
    /// A colour literal.
    Color,
    /// A bare name: a function, a constant or a keyword.
    Name(String),
    /// A `$reference`, without its `$`.
    Reference(String),
    /// An operator or a bracket, as written: `+`, `)`, `&&`.
    Symbol(&'static str),
    /// The end of the expression.
    End,
}

impl fmt::Display for Found {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Found::Number => f.write_str("a number"),
            Found::Text => f.write_str("a text"),
            Found::Color => f.write_str("a colour"),
            Found::Name(name) => write!(f, "`{name}`"),
            Found::Reference(dotted) => write!(f, "`${dotted}`"),
            Found::Symbol(symbol) => write!(f, "`{symbol}`"),
            Found::End => f.write_str("the expression ended"),
        }
    }
}

/// The bracket a missing closing symbol belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Closing {
    /// A `{…}` list.
    List,
    /// A `(…)` or `[…]` group.
    Group,
    /// The arguments of a call.
    Call,
}

impl fmt::Display for Closing {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Closing::List => "to close the list",
            Closing::Group => "to close the bracket",
            Closing::Call => "to close the call",
        })
    }
}

/// The part of an expression that wanted a type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Context {
    /// An operand of this operator, written as in the source: `+`, `&&`, `%`.
    Operator(&'static str),
    /// The right side of a `+` whose left side is a text.
    AddAfterText,
    /// The first argument of `if`.
    IfCondition,
}

impl fmt::Display for Context {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Context::Operator(symbol) => write!(f, "`{symbol}`"),
            Context::AddAfterText => f.write_str("`+` after a text"),
            Context::IfCondition => f.write_str("the condition of `if`"),
        }
    }
}

/// The type of value a function's implementation asked for from its [`crate::Arguments`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    Number,
    Text,
    Bool,
    Color,
    List,
}

impl fmt::Display for ValueKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ValueKind::Number => "a number",
            ValueKind::Text => "a text",
            ValueKind::Bool => "a bool",
            ValueKind::Color => "a colour",
            ValueKind::List => "a list",
        })
    }
}

/// What went wrong with an expression, with the values the English wording is built from.
///
/// Every variant's fields are public, and each is a plain value or implements [`fmt::Display`] ([`Type`], [`Pattern`], [`Signature`], [`Reference`]), so a host maps [`ErrorCode::key`] and the fields to an entry in its own catalogue. [`fmt::Display`] is the English wording, for hosts that do not localize.
///
/// The enum is `#[non_exhaustive]`: a later version may report a new failure, and a host's catalogue falls back to the English text for a key it does not know.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorCode {
    /// A number literal that does not parse: `1e`.
    InvalidNumber { written: String },
    /// A number literal beyond what a number can hold.
    NumberTooLarge { written: String },
    /// A `$` with no name after it.
    ReferenceNeedsName,
    /// A text literal that runs to the end of the source.
    UnclosedText { quote: char },
    /// A backslash followed by something that is not an escape.
    UnknownEscape,
    /// A `#` literal that is not a colour.
    InvalidColor { written: String },
    /// `=`, `&` or `|` alone, with the operator that was probably meant.
    LoneOperator {
        found: char,
        suggestion: &'static str,
    },
    /// A character the language gives no meaning.
    UnexpectedCharacter { found: char },
    /// A source with nothing in it.
    EmptyExpression,
    /// A complete expression followed by more.
    TrailingToken { found: Found },
    /// A closing symbol was needed; `expected` is the symbol.
    ExpectedClosing {
        expected: &'static str,
        closing: Closing,
        found: Found,
    },
    /// An operand was needed.
    ExpectedValue { found: Found },
    /// Nesting deeper than [`crate::MAX_DEPTH`].
    TooDeep,
    /// `if` without its brackets.
    IfNeedsCall,
    /// `if` with other than three arguments.
    IfArity,
    /// A whole expression whose type is not the one required of it.
    ExpectedType { expected: Type, found: Type },
    /// An operand of the wrong type.
    OperandType {
        context: Context,
        expected: Type,
        found: Type,
    },
    /// The two branches of an `if` give different types.
    BranchMismatch { first: Type, second: Type },
    /// A list item that does not share the type of the ones before it.
    ListItemMismatch { item: Type, list: Type },
    /// A function named without calling it.
    NameIsFunction { name: String },
    /// A name that is no constant, function or keyword.
    UnknownName { name: String },
    /// A constant called with brackets.
    NameIsConstant { name: String },
    /// A call to a function nobody registered.
    UnknownFunction { name: String },
    /// `==`, `<` and the like between types that cannot meet.
    CompareMismatch {
        operator: &'static str,
        left: Type,
        right: Type,
    },
    /// An ordering operator on a type that has no order.
    NotOrderable { operator: &'static str, found: Type },
    /// A function with several forms, none of which fits: `given` are the argument types, `forms` the signatures it has.
    NoMatchingForm {
        name: String,
        given: Vec<Type>,
        forms: Vec<Signature>,
    },
    /// An argument of the wrong type; `index` counts from zero.
    ArgumentType {
        name: String,
        index: usize,
        expected: Pattern,
        found: Type,
    },
    /// The wrong number of arguments: the function takes `takes` (at least that many when `at_least`).
    ArgumentCount {
        name: String,
        takes: usize,
        at_least: bool,
        given: usize,
    },
    /// A reference whose reading is of another type than its declaration.
    ReferenceType {
        reference: Reference,
        declared: Type,
        found: Type,
    },
    /// A reference that read a number that is not finite where `declared` was declared.
    ReferenceNotFinite {
        reference: Reference,
        declared: Type,
    },
    /// A function's result is not of the type its signature declares.
    ResultType {
        name: String,
        declared: Type,
        found: Type,
    },
    /// A function whose number result is not finite.
    NoFiniteResult { name: String },
    /// A division whose divisor is zero.
    DivisionByZero,
    /// An arithmetic result that is not a real number.
    NoRealResult,
    /// An arithmetic result beyond what a number can hold.
    ResultTooLarge,
    /// A text longer than [`crate::MAX_TEXT_BYTES`] would result; `limit` is that bound.
    TextTooLong { limit: usize },
    /// A value of another type reached an operation the checker had approved: a host's reading or function contradicting its declaration.
    MistypedValue,
    /// A function's argument that is not there.
    MissingArgument { index: usize },
    /// A function's argument that is not of the kind its implementation reads.
    ExpectedArgument { expected: ValueKind },
    /// A `fmt` template with a placeholder that is not `{}`, `{N}` or `{{`.
    MalformedFormat,
    /// A `fmt` placeholder with no value to fill it.
    FormatMissingValue { index: usize, given: usize },
    /// `round` with a fractional number of decimals.
    DecimalsNotWhole,
    /// `at` with a fractional position.
    PositionNotWhole,
    /// `at` with a position outside the list.
    PositionOutside { position: i64, len: usize },
    /// A failure the host reported.
    Host(HostError),
}

impl ErrorCode {
    /// The stable snake_case id of this failure, for a catalogue's key. For [`ErrorCode::Host`] it is `"host"`: the host's own id is in [`HostError::key`].
    pub fn key(&self) -> &'static str {
        match self {
            ErrorCode::InvalidNumber { .. } => "invalid_number",
            ErrorCode::NumberTooLarge { .. } => "number_too_large",
            ErrorCode::ReferenceNeedsName => "reference_needs_name",
            ErrorCode::UnclosedText { .. } => "unclosed_text",
            ErrorCode::UnknownEscape => "unknown_escape",
            ErrorCode::InvalidColor { .. } => "invalid_color",
            ErrorCode::LoneOperator { .. } => "lone_operator",
            ErrorCode::UnexpectedCharacter { .. } => "unexpected_character",
            ErrorCode::EmptyExpression => "empty_expression",
            ErrorCode::TrailingToken { .. } => "trailing_token",
            ErrorCode::ExpectedClosing { .. } => "expected_closing",
            ErrorCode::ExpectedValue { .. } => "expected_value",
            ErrorCode::TooDeep => "too_deep",
            ErrorCode::IfNeedsCall => "if_needs_call",
            ErrorCode::IfArity => "if_arity",
            ErrorCode::ExpectedType { .. } => "expected_type",
            ErrorCode::OperandType { .. } => "operand_type",
            ErrorCode::BranchMismatch { .. } => "branch_mismatch",
            ErrorCode::ListItemMismatch { .. } => "list_item_mismatch",
            ErrorCode::NameIsFunction { .. } => "name_is_function",
            ErrorCode::UnknownName { .. } => "unknown_name",
            ErrorCode::NameIsConstant { .. } => "name_is_constant",
            ErrorCode::UnknownFunction { .. } => "unknown_function",
            ErrorCode::CompareMismatch { .. } => "compare_mismatch",
            ErrorCode::NotOrderable { .. } => "not_orderable",
            ErrorCode::NoMatchingForm { .. } => "no_matching_form",
            ErrorCode::ArgumentType { .. } => "argument_type",
            ErrorCode::ArgumentCount { .. } => "argument_count",
            ErrorCode::ReferenceType { .. } => "reference_type",
            ErrorCode::ReferenceNotFinite { .. } => "reference_not_finite",
            ErrorCode::ResultType { .. } => "result_type",
            ErrorCode::NoFiniteResult { .. } => "no_finite_result",
            ErrorCode::DivisionByZero => "division_by_zero",
            ErrorCode::NoRealResult => "no_real_result",
            ErrorCode::ResultTooLarge => "result_too_large",
            ErrorCode::TextTooLong { .. } => "text_too_long",
            ErrorCode::MistypedValue => "mistyped_value",
            ErrorCode::MissingArgument { .. } => "missing_argument",
            ErrorCode::ExpectedArgument { .. } => "expected_argument",
            ErrorCode::MalformedFormat => "malformed_format",
            ErrorCode::FormatMissingValue { .. } => "format_missing_value",
            ErrorCode::DecimalsNotWhole => "decimals_not_whole",
            ErrorCode::PositionNotWhole => "position_not_whole",
            ErrorCode::PositionOutside { .. } => "position_outside",
            ErrorCode::Host(_) => "host",
        }
    }

    pub(crate) fn text_too_long() -> Self {
        ErrorCode::TextTooLong {
            limit: crate::MAX_TEXT_BYTES,
        }
    }
}

fn was_or_were(count: usize) -> &'static str {
    if count == 1 { "was" } else { "were" }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorCode::InvalidNumber { written } => write!(f, "`{written}` is not a number"),
            ErrorCode::NumberTooLarge { written } => write!(f, "`{written}` is too large"),
            ErrorCode::ReferenceNeedsName => {
                f.write_str("`$` must be followed by a name, as in `$battery.level`")
            }
            ErrorCode::UnclosedText { quote } => {
                write!(f, "this text is never closed with {quote}")
            }
            ErrorCode::UnknownEscape => {
                f.write_str("unknown escape; write \\n, \\t, \\\\, \\\" or \\'")
            }
            ErrorCode::InvalidColor { written } => write!(
                f,
                "`{written}` is not a colour; write #rgb, #rgba, #rrggbb or #rrggbbaa"
            ),
            ErrorCode::LoneOperator { found, suggestion } => {
                let how = if *found == '=' { "compare with" } else { "write" };
                write!(f, "`{found}` is not an operator; {how} `{suggestion}`")
            }
            ErrorCode::UnexpectedCharacter { found } => write!(f, "`{found}` has no meaning here"),
            ErrorCode::EmptyExpression => f.write_str("the expression is empty"),
            ErrorCode::TrailingToken { found } => {
                write!(f, "{found} cannot follow a complete expression")
            }
            ErrorCode::ExpectedClosing {
                expected,
                closing,
                found,
            } => match found {
                Found::End => write!(f, "expected `{expected}` {closing}, but {found}"),
                _ => write!(f, "expected `{expected}` {closing}, but found {found}"),
            },
            ErrorCode::ExpectedValue { found } => match found {
                Found::End => write!(f, "expected a value, but {found}"),
                _ => write!(f, "expected a value, but found {found}"),
            },
            ErrorCode::TooDeep => f.write_str("the expression nests too deeply"),
            ErrorCode::IfNeedsCall => f.write_str("`if` is written if(condition, then, else)"),
            ErrorCode::IfArity => {
                f.write_str("`if` takes three arguments: if(condition, then, else)")
            }
            ErrorCode::ExpectedType { expected, found } => {
                write!(f, "expected {expected}, but this gives {found}")
            }
            ErrorCode::OperandType {
                context,
                expected,
                found,
            } => write!(f, "{context} needs {expected}, but this is {found}"),
            ErrorCode::BranchMismatch { first, second } => write!(
                f,
                "both branches of `if` must give the same type, but the first gives {first} and this one {second}"
            ),
            ErrorCode::ListItemMismatch { item, list } => write!(
                f,
                "the items of a list must share one type, but this is {item} among {list}"
            ),
            ErrorCode::NameIsFunction { name } => {
                write!(f, "`{name}` is a function; call it as {name}(…)")
            }
            ErrorCode::UnknownName { name } => write!(f, "unknown name `{name}`"),
            ErrorCode::NameIsConstant { name } => {
                write!(f, "`{name}` is a constant; write it without brackets")
            }
            ErrorCode::UnknownFunction { name } => write!(f, "unknown function `{name}`"),
            ErrorCode::CompareMismatch {
                operator,
                left,
                right,
            } => write!(f, "`{operator}` cannot compare {left} with {right}"),
            ErrorCode::NotOrderable { operator, found } => {
                write!(f, "`{operator}` compares numbers or texts, not {found}")
            }
            ErrorCode::NoMatchingForm { name, given, forms } => {
                let given: Vec<String> = given.iter().map(ToString::to_string).collect();
                let accepted: Vec<String> =
                    forms.iter().map(|form| format!("{name}{form}")).collect();
                write!(
                    f,
                    "no form of `{name}` takes ({}); it takes {}",
                    given.join(", "),
                    accepted.join(" or ")
                )
            }
            ErrorCode::ArgumentType {
                name,
                index,
                expected,
                found,
            } => write!(
                f,
                "argument {} of `{name}` must be {expected}, but this is {found}",
                index + 1
            ),
            ErrorCode::ArgumentCount {
                name,
                takes,
                at_least,
                given,
            } => {
                let noun = if *takes == 1 { "argument" } else { "arguments" };
                let least = if *at_least { "at least " } else { "" };
                write!(
                    f,
                    "`{name}` takes {least}{takes} {noun}, but {given} {} given",
                    was_or_were(*given)
                )
            }
            ErrorCode::ReferenceType {
                reference,
                declared,
                found,
            } => write!(f, "{reference} gave {found} where {declared} was declared"),
            ErrorCode::ReferenceNotFinite {
                reference,
                declared,
            } => write!(
                f,
                "{reference} gave a number that is not finite where {declared} was declared"
            ),
            ErrorCode::ResultType {
                name,
                declared,
                found,
            } => write!(f, "`{name}` gave {found} where {declared} was declared"),
            ErrorCode::NoFiniteResult { name } => {
                write!(f, "`{name}` has no finite result for these arguments")
            }
            ErrorCode::DivisionByZero => f.write_str("division by zero"),
            ErrorCode::NoRealResult => f.write_str("this has no real-number result"),
            ErrorCode::ResultTooLarge => f.write_str("the result is too large"),
            ErrorCode::TextTooLong { limit } => {
                write!(f, "the text would be longer than {limit} bytes")
            }
            ErrorCode::MistypedValue => {
                f.write_str("a value of the wrong type reached this point")
            }
            ErrorCode::MissingArgument { index } => write!(f, "argument {} is missing", index + 1),
            ErrorCode::ExpectedArgument { expected } => write!(f, "expected {expected}"),
            ErrorCode::MalformedFormat => f.write_str(
                "a placeholder in the format must be {}, {N} or {{, and a lone } must be written }}",
            ),
            ErrorCode::FormatMissingValue { index, given } => write!(
                f,
                "the format asks for value {index}, but {given} {} given",
                was_or_were(*given)
            ),
            ErrorCode::DecimalsNotWhole => f.write_str("the number of decimals must be whole"),
            ErrorCode::PositionNotWhole => f.write_str("a position must be a whole number"),
            ErrorCode::PositionOutside { position, len } => {
                write!(f, "position {position} is outside a list of {len}")
            }
            ErrorCode::Host(error) => error.fmt(f),
        }
    }
}

impl From<HostError> for ErrorCode {
    fn from(error: HostError) -> Self {
        ErrorCode::Host(error)
    }
}
