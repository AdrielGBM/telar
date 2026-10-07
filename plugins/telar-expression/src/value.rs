//! [`Value`]: what an expression evaluates to, and [`format_number`], how a number reads as text.

use std::fmt;
use std::sync::Arc;

use geometry_core::Color;

use crate::Type;

/// An evaluated value. Cheap to clone: text and lists are shared, since a bound value is read again on every frame that draws it.
///
/// A number is always finite: the evaluator turns an infinity or a NaN into an error at the operation that produced it.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Number(f64),
    Text(Arc<str>),
    Bool(bool),
    Color(Color),
    List(Arc<[Value]>),
}

impl Value {
    pub fn text(text: impl Into<Arc<str>>) -> Self {
        Self::Text(text.into())
    }

    pub fn list(items: impl IntoIterator<Item = Value>) -> Self {
        Self::List(items.into_iter().collect())
    }

    /// The empty value of `ty`: zero, empty text, false, a transparent colour, an empty list. What a host shows in place of a reading it must not reveal. [`Type::Never`] has none.
    pub fn empty(ty: &Type) -> Option<Self> {
        Some(match ty {
            Type::Number => Self::Number(0.0),
            Type::Text => Self::text(""),
            Type::Bool => Self::Bool(false),
            Type::Color => Self::Color(Color::TRANSPARENT),
            Type::List(_) => Self::list([]),
            Type::Never => return None,
        })
    }

    /// The type this value has. A list takes its first element's, and an empty one is a list of [`Type::Never`].
    pub fn type_of(&self) -> Type {
        match self {
            Value::Number(_) => Type::Number,
            Value::Text(_) => Type::Text,
            Value::Bool(_) => Type::Bool,
            Value::Color(_) => Type::Color,
            Value::List(items) => Type::list(items.first().map_or(Type::Never, Value::type_of)),
        }
    }

    /// Whether this value is a valid `ty`: the right shape all the way down, and every number finite.
    pub fn conforms_to(&self, ty: &Type) -> bool {
        match (self, ty) {
            (Value::Number(n), Type::Number) => n.is_finite(),
            (Value::Text(_), Type::Text)
            | (Value::Bool(_), Type::Bool)
            | (Value::Color(_), Type::Color) => true,
            (Value::List(items), Type::List(element)) => {
                items.iter().all(|item| item.conforms_to(element))
            }
            _ => false,
        }
    }

    pub fn as_number(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            Value::Text(text) => Some(text),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_color(&self) -> Option<Color> {
        match self {
            Value::Color(color) => Some(*color),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<&[Value]> {
        match self {
            Value::List(items) => Some(items),
            _ => None,
        }
    }
}

/// How a value reads when it becomes text, in `fmt`, `join` or a text property: numbers through [`format_number`], colours as `#rrggbb` (`#rrggbbaa` when not opaque), lists as their items separated by `, `.
impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Number(n) => f.write_str(&format_number(*n)),
            Value::Text(text) => f.write_str(text),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Color(color) => {
                let [r, g, b, a] = color.to_rgba8();
                write!(f, "#{r:02x}{g:02x}{b:02x}")?;
                if a != u8::MAX {
                    write!(f, "{a:02x}")?;
                }
                Ok(())
            }
            Value::List(items) => {
                for (index, item) in items.iter().enumerate() {
                    if index > 0 {
                        f.write_str(", ")?;
                    }
                    item.fmt(f)?;
                }
                Ok(())
            }
        }
    }
}

impl From<f64> for Value {
    fn from(n: f64) -> Self {
        Self::Number(n)
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Self::Bool(b)
    }
}

impl From<&str> for Value {
    fn from(text: &str) -> Self {
        Self::text(text)
    }
}

impl From<String> for Value {
    fn from(text: String) -> Self {
        Self::text(text)
    }
}

impl From<Color> for Value {
    fn from(color: Color) -> Self {
        Self::Color(color)
    }
}

/// A number the way a calculator shows it: no trailing `.0` on a whole number, and rounded to ten decimals so binary noise stays hidden (`0.1 + 0.2` reads `0.3`).
pub fn format_number(value: f64) -> String {
    let scaled = value * 1e10;
    let rounded = if scaled.is_finite() {
        scaled.round() / 1e10
    } else {
        value
    };
    if rounded == rounded.trunc() && rounded.abs() < 1e15 {
        return format!("{}", rounded as i64);
    }
    let text = format!("{rounded}");
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        text
    }
}

/// The longest text an expression may build, in bytes. Building past it is an evaluation error, since `replace` multiplies a text's length and nesting it would otherwise exhaust memory.
pub const MAX_TEXT_BYTES: usize = 1 << 20;
