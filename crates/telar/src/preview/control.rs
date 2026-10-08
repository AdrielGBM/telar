//! [`ControlKind`]: which control an arg is edited with.

use super::ArgValue;

/// The control an arg gets, decided by its type and refined by the props it feeds.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ControlKind {
    Toggle,
    Number(NumberControl),
    Text {
        multiline: bool,
    },
    Color,
    /// One of a fixed set of names: an enum's variants, in declaration order.
    Choice {
        variants: &'static [&'static str],
    },
    /// The inner control plus a way to leave the value unset.
    Optional(&'static ControlKind),
    /// Shown, never edited: a value whose type has no control.
    ReadOnly,
}

impl ControlKind {
    pub const INTEGER: Self = Self::Number(NumberControl::INTEGER);
    pub const FLOAT: Self = Self::Number(NumberControl::FLOAT);
    pub const TEXT: Self = Self::Text { multiline: false };
    pub const MULTILINE: Self = Self::Text { multiline: true };

    /// The same control limited to `min..=max`, which turns a number field into a slider. Anything but a number is returned as it was.
    pub const fn range(self, min: f64, max: f64) -> Self {
        match self {
            Self::Number(number) => Self::Number(number.range(min, max)),
            other => other,
        }
    }

    /// The same control moving in increments of `step`. Anything but a number is returned as it was.
    pub const fn step(self, step: f64) -> Self {
        match self {
            Self::Number(number) => Self::Number(number.step(step)),
            other => other,
        }
    }

    /// The same control with a text field grown to several lines. Anything but text is returned as it was.
    pub const fn multiline(self) -> Self {
        match self {
            Self::Text { .. } => Self::MULTILINE,
            other => other,
        }
    }

    /// Whether a choice is short enough to show every option at once, as segments rather than a menu.
    pub const fn is_segmented(&self) -> bool {
        matches!(self, Self::Choice { variants } if variants.len() <= 4)
    }

    pub const fn is_read_only(&self) -> bool {
        matches!(self, Self::ReadOnly)
    }

    /// Whether `other` edits the same kind of value, whatever its limits: two number fields of one sort, or two choices of the same variants.
    pub fn is_same_kind(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Number(a), Self::Number(b)) => a.integer == b.integer,
            (Self::Choice { variants: a }, Self::Choice { variants: b }) => a == b,
            (Self::Optional(a), Self::Optional(b)) => a.is_same_kind(b),
            (a, b) => std::mem::discriminant(a) == std::mem::discriminant(b),
        }
    }

    /// Whether this control can show `value`.
    pub fn admits(&self, value: &ArgValue) -> bool {
        match (self, value) {
            (Self::Toggle, ArgValue::Bool(_)) => true,
            (Self::Number(_), ArgValue::Int(_)) => true,
            (Self::Number(number), ArgValue::Float(value)) => {
                !number.integer || value.fract() == 0.0
            }
            (Self::Text { .. }, ArgValue::Text(_)) | (Self::Color, ArgValue::Color(_)) => true,
            (Self::Choice { variants }, ArgValue::Choice(name)) => {
                variants.contains(&name.as_str())
            }
            (Self::Optional(_), ArgValue::Unset) => true,
            (Self::Optional(inner), value) => inner.admits(value),
            _ => false,
        }
    }
}

/// A number control's limits.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NumberControl {
    /// Whole numbers only.
    pub integer: bool,
    /// `(min, max)`, inclusive. A range is what makes the control a slider.
    pub range: Option<(f64, f64)>,
    pub step: Option<f64>,
}

impl NumberControl {
    pub const INTEGER: Self = Self {
        integer: true,
        range: None,
        step: None,
    };
    pub const FLOAT: Self = Self {
        integer: false,
        range: None,
        step: None,
    };

    pub const fn range(self, min: f64, max: f64) -> Self {
        Self {
            range: Some((min, max)),
            ..self
        }
    }

    pub const fn step(self, step: f64) -> Self {
        Self {
            step: Some(step),
            ..self
        }
    }
}
