//! The evaluator: a checked tree and a [`Resolver`] into a [`Value`], or an [`Error`] — never a panic.

use crate::compile::{Arithmetic, Comparison, Node, NodeKind};
use crate::registry::Arguments;
use crate::{Error, ErrorCode, MAX_TEXT_BYTES, Resolver, Span, Value};

pub(crate) fn evaluate(node: &Node, resolver: &dyn Resolver) -> Result<Value, Error> {
    let span = node.span;
    Ok(match &node.kind {
        NodeKind::Literal(value) => value.clone(),
        NodeKind::List(items) => Value::List(
            items
                .iter()
                .map(|item| evaluate(item, resolver))
                .collect::<Result<_, _>>()?,
        ),
        NodeKind::Reference(reference, ty) => {
            let value = resolver
                .read(reference)
                .map_err(|error| Error::evaluation(span, ErrorCode::Host(error)))?;
            if !value.conforms_to(ty) {
                let code = match value {
                    Value::Number(_) => ErrorCode::ReferenceNotFinite {
                        reference: reference.clone(),
                        declared: ty.clone(),
                    },
                    _ => ErrorCode::ReferenceType {
                        reference: reference.clone(),
                        declared: ty.clone(),
                        found: value.type_of(),
                    },
                };
                return Err(Error::evaluation(span, code));
            }
            value
        }
        NodeKind::Negate(inner) => Value::Number(-number(inner, resolver)?),
        NodeKind::Not(inner) => Value::Bool(!boolean(inner, resolver)?),
        NodeKind::Hundredth(inner) => Value::Number(number(inner, resolver)? / 100.0),
        NodeKind::Arithmetic(op, left, right) => {
            let left = number(left, resolver)?;
            let divisor_span = right.span;
            let right = number(right, resolver)?;
            let result = match op {
                Arithmetic::Add => left + right,
                Arithmetic::Subtract => left - right,
                Arithmetic::Multiply => left * right,
                Arithmetic::Divide if right == 0.0 => {
                    return Err(Error::evaluation(divisor_span, ErrorCode::DivisionByZero));
                }
                Arithmetic::Divide => left / right,
                Arithmetic::Power => left.powf(right),
            };
            Value::Number(finite(result, span)?)
        }
        NodeKind::Relative {
            subtract,
            base,
            percent,
        } => {
            let base = number(base, resolver)?;
            let change = base * number(percent, resolver)?;
            let result = if *subtract {
                base - change
            } else {
                base + change
            };
            Value::Number(finite(result, span)?)
        }
        NodeKind::Concat(left, right) => {
            let left = evaluate(left, resolver)?;
            let right = evaluate(right, resolver)?;
            let (Some(left), Some(right)) = (left.as_text(), right.as_text()) else {
                return Err(mistyped(span));
            };
            if left.len() + right.len() > MAX_TEXT_BYTES {
                return Err(Error::evaluation(span, ErrorCode::text_too_long()));
            }
            Value::text(format!("{left}{right}"))
        }
        NodeKind::Compare(comparison, left, right) => {
            let left = evaluate(left, resolver)?;
            let right = evaluate(right, resolver)?;
            let ordering = match (&left, &right) {
                (Value::Number(a), Value::Number(b)) => a.partial_cmp(b),
                (Value::Text(a), Value::Text(b)) => Some(a.cmp(b)),
                _ => None,
            }
            .ok_or_else(|| mistyped(span))?;
            Value::Bool(match comparison {
                Comparison::Less => ordering.is_lt(),
                Comparison::LessEqual => ordering.is_le(),
                Comparison::Greater => ordering.is_gt(),
                Comparison::GreaterEqual => ordering.is_ge(),
            })
        }
        NodeKind::Equal {
            negate,
            left,
            right,
        } => {
            let equal = evaluate(left, resolver)? == evaluate(right, resolver)?;
            Value::Bool(equal != *negate)
        }
        NodeKind::And(left, right) => {
            Value::Bool(boolean(left, resolver)? && boolean(right, resolver)?)
        }
        NodeKind::Or(left, right) => {
            Value::Bool(boolean(left, resolver)? || boolean(right, resolver)?)
        }
        NodeKind::If {
            condition,
            then,
            otherwise,
        } => {
            if boolean(condition, resolver)? {
                evaluate(then, resolver)?
            } else {
                evaluate(otherwise, resolver)?
            }
        }
        NodeKind::Call {
            name,
            implementation,
            returns,
            arguments,
        } => {
            let values = arguments
                .iter()
                .map(|argument| evaluate(argument, resolver))
                .collect::<Result<Vec<_>, _>>()?;
            let value = implementation(Arguments(&values)).map_err(|error| {
                let at = error
                    .argument
                    .and_then(|index| arguments.get(index))
                    .map_or(span, |argument| argument.span);
                Error::evaluation(at, error.code)
            })?;
            if !value.conforms_to(returns) {
                let code = match value {
                    Value::Number(_) => ErrorCode::NoFiniteResult { name: name.clone() },
                    _ => ErrorCode::ResultType {
                        name: name.clone(),
                        declared: returns.clone(),
                        found: value.type_of(),
                    },
                };
                return Err(Error::evaluation(span, code));
            }
            value
        }
        NodeKind::Invalid => return Err(mistyped(span)),
    })
}

fn number(node: &Node, resolver: &dyn Resolver) -> Result<f64, Error> {
    evaluate(node, resolver)?
        .as_number()
        .ok_or_else(|| mistyped(node.span))
}

fn boolean(node: &Node, resolver: &dyn Resolver) -> Result<bool, Error> {
    evaluate(node, resolver)?
        .as_bool()
        .ok_or_else(|| mistyped(node.span))
}

fn finite(result: f64, span: Span) -> Result<f64, Error> {
    if result.is_finite() {
        Ok(result)
    } else if result.is_nan() {
        Err(Error::evaluation(span, ErrorCode::NoRealResult))
    } else {
        Err(Error::evaluation(span, ErrorCode::ResultTooLarge))
    }
}

fn mistyped(span: Span) -> Error {
    Error::evaluation(span, ErrorCode::MistypedValue)
}

#[cfg(test)]
#[path = "eval_test.rs"]
mod tests;
