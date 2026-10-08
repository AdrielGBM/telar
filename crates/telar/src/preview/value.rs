//! [`ArgValue`]: an arg's value, erased from its Rust type, with the one text form that hot reload and deep links both carry.

use std::fmt::{self, Write as _};
use std::str::FromStr;

use serde::de::{self, Deserializer, Visitor};
use serde::{Deserialize, Serialize, Serializer};

use crate::Color;

/// An arg's value as a control edits it and a link carries it.
///
/// **The text form** is what [`fmt::Display`] writes, [`FromStr`] reads and serde stores, so a value survives a hot reload and a copied link the same way:
///
/// | Value | Text |
/// | --- | --- |
/// | `Unset` | `none` |
/// | `Bool` | `true`, `false` |
/// | `Int` | `-12` |
/// | `Float` | `1.5`, `2.0`, `1e-7`, `+inf`, `-inf`, `+NaN` |
/// | `Text` | `"Save \"all\"\n"` |
/// | `Color` | `#ff8800`, `#ff880080`, or `rgba(0.1, 0.2, 0.3, 1.0)` when a channel is not a whole byte |
/// | `Choice` | `Large` |
///
/// The forms never overlap: a number starts with a digit or a sign, text with a quote, a colour with `#` or `rgba(`, and a choice is any other identifier. A float always carries a `.`, an exponent or a sign word, so `2.0` stays a float.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub enum ArgValue {
    /// An `Option` arg holding `None`.
    Unset,
    Bool(bool),
    /// Wide enough for every integer primitive but `u128`.
    Int(i128),
    Float(f64),
    Text(String),
    Color(Color),
    /// The variant of an enum deriving `PreviewArg`, by name.
    Choice(String),
}

/// Why a text form is not an [`ArgValue`].
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArgValueParseError {
    pub text: String,
}

impl fmt::Display for ArgValueParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "`{}` is not an arg value", self.text)
    }
}

impl std::error::Error for ArgValueParseError {}

impl fmt::Display for ArgValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unset => f.write_str("none"),
            Self::Bool(value) => write!(f, "{value}"),
            Self::Int(value) => write!(f, "{value}"),
            Self::Float(value) => write_float(f, *value),
            Self::Text(text) => write_quoted(f, text),
            Self::Color(color) => write_color(f, *color),
            Self::Choice(name) => f.write_str(name),
        }
    }
}

fn write_float(f: &mut fmt::Formatter<'_>, value: f64) -> fmt::Result {
    if value.is_nan() {
        f.write_str("+NaN")
    } else if value.is_infinite() {
        f.write_str(if value > 0.0 { "+inf" } else { "-inf" })
    } else {
        write!(f, "{value:?}")
    }
}

fn write_quoted(f: &mut fmt::Formatter<'_>, text: &str) -> fmt::Result {
    f.write_char('"')?;
    for c in text.chars() {
        match c {
            '"' => f.write_str("\\\"")?,
            '\\' => f.write_str("\\\\")?,
            '\n' => f.write_str("\\n")?,
            '\r' => f.write_str("\\r")?,
            '\t' => f.write_str("\\t")?,
            c if c.is_control() => write!(f, "\\u{{{:x}}}", c as u32)?,
            c => f.write_char(c)?,
        }
    }
    f.write_char('"')
}

fn write_color(f: &mut fmt::Formatter<'_>, color: Color) -> fmt::Result {
    let bytes = color.to_rgba8();
    let exact = Color::from_rgba_u8(bytes[0], bytes[1], bytes[2], bytes[3]) == color;
    match (exact, bytes[3]) {
        (true, 255) => write!(f, "#{:02x}{:02x}{:02x}", bytes[0], bytes[1], bytes[2]),
        (true, _) => write!(
            f,
            "#{:02x}{:02x}{:02x}{:02x}",
            bytes[0], bytes[1], bytes[2], bytes[3]
        ),
        (false, _) => write!(
            f,
            "rgba({:?}, {:?}, {:?}, {:?})",
            color.r, color.g, color.b, color.a
        ),
    }
}

impl FromStr for ArgValue {
    type Err = ArgValueParseError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        parse(text).ok_or_else(|| ArgValueParseError {
            text: text.to_string(),
        })
    }
}

fn parse(text: &str) -> Option<ArgValue> {
    let first = text.chars().next()?;
    match first {
        '"' => parse_quoted(text).map(ArgValue::Text),
        '#' => Color::from_hex(text).map(ArgValue::Color),
        '0'..='9' | '-' | '+' | '.' => parse_number(text),
        _ if text.starts_with("rgba(") => parse_rgba(text).map(ArgValue::Color),
        _ => match text {
            "none" => Some(ArgValue::Unset),
            "true" => Some(ArgValue::Bool(true)),
            "false" => Some(ArgValue::Bool(false)),
            _ if is_identifier(text) => Some(ArgValue::Choice(text.to_string())),
            _ => None,
        },
    }
}

fn parse_number(text: &str) -> Option<ArgValue> {
    let digits = text.strip_prefix(['-', '+']).unwrap_or(text);
    if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
        return text.parse().ok().map(ArgValue::Int);
    }
    text.parse().ok().map(ArgValue::Float)
}

fn parse_rgba(text: &str) -> Option<Color> {
    let inner = text.strip_prefix("rgba(")?.strip_suffix(')')?;
    let mut channels = inner.split(',').map(|part| part.trim().parse::<f32>());
    let mut next = || channels.next()?.ok();
    let color = Color::rgba(next()?, next()?, next()?, next()?);
    channels.next().is_none().then_some(color)
}

fn parse_quoted(text: &str) -> Option<String> {
    let inner = text.strip_prefix('"')?.strip_suffix('"')?;
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return None,
            '\\' => out.push(match chars.next()? {
                '"' => '"',
                '\\' => '\\',
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                'u' => {
                    let rest = chars.as_str().strip_prefix('{')?;
                    let end = rest.find('}')?;
                    let code = u32::from_str_radix(&rest[..end], 16).ok()?;
                    chars = rest[end + 1..].chars();
                    char::from_u32(code)?
                }
                _ => return None,
            }),
            c => out.push(c),
        }
    }
    Some(out)
}

fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars.next().is_some_and(|c| c.is_alphabetic() || c == '_')
        && chars.all(|c| c.is_alphanumeric() || c == '_')
}

impl Serialize for ArgValue {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for ArgValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct TextForm;

        impl Visitor<'_> for TextForm {
            type Value = ArgValue;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an arg value in its text form")
            }

            fn visit_str<E: de::Error>(self, text: &str) -> Result<ArgValue, E> {
                text.parse().map_err(E::custom)
            }
        }

        deserializer.deserialize_str(TextForm)
    }
}

#[cfg(test)]
#[path = "value_test.rs"]
mod tests;
