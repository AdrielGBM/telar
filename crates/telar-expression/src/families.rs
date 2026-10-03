//! The function families this crate ships. Each is a plain function registering into a [`Registry`], which is all a host's own family needs to be too.
//!
//! The families follow KLWP's — `tc`, `mu` and the rest — but as one function per operation rather than a mode word as the first argument, so every form has a signature the checker can hold it to: KLWP's `tc(up, x)` is `upper(x)`, `mu(round, x)` is `round(x)`.

use std::fmt::Write;

use crate::{
    Arguments, CallError, ErrorCode, MAX_TEXT_BYTES, Pattern, Registry, Signature, Type, Value,
};

type Unary = (&'static str, fn(f64) -> f64, &'static str);

fn number() -> Pattern {
    Pattern::Exact(Type::Number)
}

fn text_pattern() -> Pattern {
    Pattern::Exact(Type::Text)
}

fn color_pattern() -> Pattern {
    Pattern::Exact(Type::Color)
}

fn bounded(text: String) -> Result<Value, CallError> {
    if text.len() > MAX_TEXT_BYTES {
        return Err(CallError::new(ErrorCode::text_too_long()));
    }
    Ok(Value::text(text))
}

/// Text: `fmt`, `upper`, `lower`, `trim`, `len`, `substring`, `replace`, `contains`.
///
/// `fmt(format, …values)` fills each `{}` in `format` with the next value, `{N}` with the value at `N` (from zero), and writes `{{` and `}}` as braces. Positions and lengths count characters, not bytes.
pub fn text(registry: &mut Registry) {
    let text_to_text = || Signature::new([text_pattern()], Type::Text);
    registry
        .function(
            "fmt",
            Signature::new([text_pattern()], Type::Text).rest(Pattern::Any),
            "Fills each {} in the format with the next value",
            format,
        )
        .function("upper", text_to_text(), "The text in upper case", |a| {
            bounded(a.text(0)?.to_uppercase())
        })
        .function("lower", text_to_text(), "The text in lower case", |a| {
            bounded(a.text(0)?.to_lowercase())
        })
        .function(
            "trim",
            text_to_text(),
            "The text without leading or trailing whitespace",
            |a| Ok(Value::text(a.text(0)?.trim())),
        )
        .function(
            "len",
            Signature::new([text_pattern()], Type::Number),
            "How many characters the text has",
            |a| Ok(Value::Number(a.text(0)?.chars().count() as f64)),
        )
        .function(
            "substring",
            Signature::new([text_pattern(), number()], Type::Text),
            "The characters from a position to the end",
            |a| substring(a.text(0)?, a.number(1)?, f64::INFINITY),
        )
        .function(
            "substring",
            Signature::new([text_pattern(), number(), number()], Type::Text),
            "The characters from a position up to, not including, another",
            |a| substring(a.text(0)?, a.number(1)?, a.number(2)?),
        )
        .function(
            "replace",
            Signature::new([text_pattern(), text_pattern(), text_pattern()], Type::Text),
            "The text with every occurrence of the second replaced by the third",
            replace,
        )
        .function(
            "contains",
            Signature::new([text_pattern(), text_pattern()], Type::Bool),
            "Whether the second text occurs in the first",
            |a| Ok(Value::Bool(a.text(0)?.contains(a.text(1)?))),
        );
}

fn format(arguments: Arguments<'_>) -> Result<Value, CallError> {
    let template = arguments.text(0)?;
    let values = arguments.rest(1);
    let malformed = || CallError::argument(0, ErrorCode::MalformedFormat);
    let mut out = String::new();
    let mut next = 0;
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.next_if_eq(&'{').is_some() => out.push('{'),
            '}' if chars.next_if_eq(&'}').is_some() => out.push('}'),
            '}' => return Err(malformed()),
            '{' => {
                let mut digits = String::new();
                loop {
                    match chars.next() {
                        Some('}') => break,
                        Some(digit) if digit.is_ascii_digit() => digits.push(digit),
                        _ => return Err(malformed()),
                    }
                }
                let index = if digits.is_empty() {
                    next += 1;
                    next - 1
                } else {
                    digits.parse::<usize>().map_err(|_| malformed())?
                };
                let value = values.get(index).ok_or_else(|| {
                    CallError::argument(
                        0,
                        ErrorCode::FormatMissingValue {
                            index,
                            given: values.len(),
                        },
                    )
                })?;
                let _ = write!(out, "{value}");
                if out.len() > MAX_TEXT_BYTES {
                    return Err(CallError::new(ErrorCode::text_too_long()));
                }
            }
            other => out.push(other),
        }
    }
    Ok(Value::text(out))
}

fn char_position(position: f64, len: usize) -> usize {
    if position <= 0.0 {
        0
    } else {
        (position.floor() as usize).min(len)
    }
}

fn substring(text: &str, start: f64, end: f64) -> Result<Value, CallError> {
    let len = text.chars().count();
    let start = char_position(start, len);
    let end = char_position(end, len).max(start);
    Ok(Value::text(
        text.chars()
            .skip(start)
            .take(end - start)
            .collect::<String>(),
    ))
}

fn replace(arguments: Arguments<'_>) -> Result<Value, CallError> {
    let text = arguments.text(0)?;
    let from = arguments.text(1)?;
    let to = arguments.text(2)?;
    let occurrences = if from.is_empty() {
        text.chars().count() + 1
    } else {
        text.matches(from).count()
    };
    let length = (text.len() - occurrences * from.len())
        .saturating_add(occurrences.saturating_mul(to.len()));
    if length > MAX_TEXT_BYTES {
        return Err(CallError::new(ErrorCode::text_too_long()));
    }
    Ok(Value::text(text.replace(from, to)))
}

/// Maths: the constants `pi` (also `π`), `e` and `tau`, and `sqrt`, `cbrt`, `abs`, `floor`, `ceil`, `round` (to a whole number, or to N decimals), `ln`, `log` (base 10, also `log10`), `log2`, `exp`, `sin`, `cos`, `tan`, `asin`, `acos`, `atan` (radians), `pow`, `min` and `max`.
pub fn math(registry: &mut Registry) {
    use std::f64::consts::{E, PI, TAU};
    registry
        .constant(
            "pi",
            Value::Number(PI),
            "The ratio of a circle's circumference to its diameter",
        )
        .constant(
            "π",
            Value::Number(PI),
            "The ratio of a circle's circumference to its diameter",
        )
        .constant(
            "e",
            Value::Number(E),
            "Euler's number, the base of the natural logarithm",
        )
        .constant("tau", Value::Number(TAU), "A full turn in radians, 2π");
    let unary: [Unary; 17] = [
        ("sqrt", f64::sqrt, "The square root"),
        ("cbrt", f64::cbrt, "The cube root"),
        ("abs", f64::abs, "The number without its sign"),
        ("floor", f64::floor, "The largest whole number not above it"),
        ("ceil", f64::ceil, "The smallest whole number not below it"),
        (
            "round",
            f64::round,
            "The nearest whole number, halves away from zero",
        ),
        ("ln", f64::ln, "The natural logarithm"),
        ("log", f64::log10, "The base-10 logarithm"),
        ("log10", f64::log10, "The base-10 logarithm"),
        ("log2", f64::log2, "The base-2 logarithm"),
        ("exp", f64::exp, "e raised to the number"),
        ("sin", f64::sin, "The sine of an angle in radians"),
        ("cos", f64::cos, "The cosine of an angle in radians"),
        ("tan", f64::tan, "The tangent of an angle in radians"),
        ("asin", f64::asin, "The angle in radians whose sine this is"),
        (
            "acos",
            f64::acos,
            "The angle in radians whose cosine this is",
        ),
        (
            "atan",
            f64::atan,
            "The angle in radians whose tangent this is",
        ),
    ];
    for (name, apply, summary) in unary {
        registry.function(
            name,
            Signature::new([number()], Type::Number),
            summary,
            move |a| Ok(Value::Number(apply(a.number(0)?))),
        );
    }
    let variadic = || Signature::new([number()], Type::Number).rest(number());
    registry
        .function(
            "round",
            Signature::new([number(), number()], Type::Number),
            "Rounded to the given number of decimals",
            |a| {
                let digits = a.number(1)?;
                if digits.fract() != 0.0 {
                    return Err(CallError::argument(1, ErrorCode::DecimalsNotWhole));
                }
                let scale = 10f64.powf(digits);
                Ok(Value::Number((a.number(0)? * scale).round() / scale))
            },
        )
        .function(
            "pow",
            Signature::new([number(), number()], Type::Number),
            "The first number raised to the second",
            |a| Ok(Value::Number(a.number(0)?.powf(a.number(1)?))),
        )
        .function("min", variadic(), "The smallest of the numbers", |a| {
            extreme(a, f64::min)
        })
        .function("max", variadic(), "The largest of the numbers", |a| {
            extreme(a, f64::max)
        });
}

fn extreme(arguments: Arguments<'_>, pick: fn(f64, f64) -> f64) -> Result<Value, CallError> {
    let mut best = arguments.number(0)?;
    for index in 1..arguments.len() {
        best = pick(best, arguments.number(index)?);
    }
    Ok(Value::Number(best))
}

/// Lists: `len`, `at` (from zero, and from the end when negative: `-1` is the last item) and `join`.
pub fn list(registry: &mut Registry) {
    let any_list = || Pattern::list(Pattern::Any);
    registry
        .function(
            "len",
            Signature::new([any_list()], Type::Number),
            "How many items the list has",
            |a| Ok(Value::Number(a.list(0)?.len() as f64)),
        )
        .function(
            "at",
            Signature::new([Pattern::list(Pattern::Var(0)), number()], Pattern::Var(0)),
            "The item at a position, counting from zero; negative counts from the end",
            at,
        )
        .function(
            "join",
            Signature::new([any_list(), text_pattern()], Type::Text),
            "The items as text, with the separator between each",
            join,
        );
}

fn at(arguments: Arguments<'_>) -> Result<Value, CallError> {
    let items = arguments.list(0)?;
    let position = arguments.number(1)?;
    if position.fract() != 0.0 {
        return Err(CallError::argument(1, ErrorCode::PositionNotWhole));
    }
    let len = items.len() as f64;
    let index = if position < 0.0 {
        len + position
    } else {
        position
    };
    if index < 0.0 || index >= len {
        return Err(CallError::argument(
            1,
            ErrorCode::PositionOutside {
                position: position as i64,
                len: items.len(),
            },
        ));
    }
    Ok(items[index as usize].clone())
}

fn join(arguments: Arguments<'_>) -> Result<Value, CallError> {
    let items = arguments.list(0)?;
    let separator = arguments.text(1)?;
    let mut out = String::new();
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            out.push_str(separator);
        }
        let _ = write!(out, "{item}");
        if out.len() > MAX_TEXT_BYTES {
            return Err(CallError::new(ErrorCode::text_too_long()));
        }
    }
    Ok(Value::text(out))
}

/// Colour: `mix` (in Oklch, so the middle of two saturated colours stays saturated), `alpha` and `contrast` (the WCAG ratio, 1 to 21). Fractions are clamped to `0..=1`, so `mix(a, b, 30%)` is three tenths of the way.
pub fn color(registry: &mut Registry) {
    registry
        .function(
            "mix",
            Signature::new([color_pattern(), color_pattern(), number()], Type::Color),
            "The colour a fraction of the way from the first to the second",
            |a| {
                let t = a.number(2)?.clamp(0.0, 1.0) as f32;
                Ok(Value::Color(a.color(0)?.mix(a.color(1)?, t)))
            },
        )
        .function(
            "alpha",
            Signature::new([color_pattern(), number()], Type::Color),
            "The colour with its opacity set, from 0 to 1",
            |a| {
                let alpha = a.number(1)?.clamp(0.0, 1.0) as f32;
                Ok(Value::Color(a.color(0)?.with_alpha(alpha)))
            },
        )
        .function(
            "contrast",
            Signature::new([color_pattern(), color_pattern()], Type::Number),
            "The WCAG contrast ratio between two colours, from 1 to 21",
            |a| {
                Ok(Value::Number(f64::from(
                    a.color(0)?.contrast_ratio(a.color(1)?),
                )))
            },
        );
}

#[cfg(test)]
#[path = "families_test.rs"]
mod tests;
