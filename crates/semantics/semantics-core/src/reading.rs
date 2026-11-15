use std::hash::{Hash, Hasher};
use std::sync::Arc;

/// A numeric control's reading: where it is now, and the range that makes that number mean something.
///
/// Compared and hashed by the bits of its numbers, so a box carrying one can still be part of a [`Semantics`](crate::Semantics) that is compared and hashed as a whole.
#[derive(Debug, Clone, Copy)]
pub struct NumericValue {
    pub now: f64,
    pub min: f64,
    pub max: f64,
}

impl NumericValue {
    fn bits(&self) -> [u64; 3] {
        [self.now.to_bits(), self.min.to_bits(), self.max.to_bits()]
    }
}

impl PartialEq for NumericValue {
    fn eq(&self, other: &Self) -> bool {
        self.bits() == other.bits()
    }
}

impl Eq for NumericValue {}

impl Hash for NumericValue {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.bits().hash(state);
    }
}

/// What a control shows a reader as its value: where a slider stands, what a field holds, which choice a select has made.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Value {
    /// A number between bounds, and the words a reader says for it where the bare number would not say it: "Medium" for 2 of 3, "40%" for 0.4.
    Number {
        reading: NumericValue,
        text: Option<Arc<str>>,
    },
    /// Text: what a field holds, the label of the choice a select has made.
    Text(Arc<str>),
}

impl From<NumericValue> for Value {
    fn from(reading: NumericValue) -> Self {
        Self::Number {
            reading,
            text: None,
        }
    }
}

impl Value {
    /// The number, for a value that is one.
    pub fn reading(&self) -> Option<NumericValue> {
        match self {
            Self::Number { reading, .. } => Some(*reading),
            Self::Text(_) => None,
        }
    }

    /// The words a reader says for the value, where it has any: a number's own words, or the text itself.
    pub fn text(&self) -> Option<&str> {
        match self {
            Self::Number { text, .. } => text.as_deref(),
            Self::Text(text) => Some(text),
        }
    }
}

/// The axis a control runs along, as ARIA's `aria-orientation` says it.
///
/// For a splitter this is the axis of the *bar itself*: a vertical bar sits between two panes side by side and is dragged left and right.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Orientation {
    Horizontal,
    Vertical,
}

impl Orientation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Horizontal => "horizontal",
            Self::Vertical => "vertical",
        }
    }
}

/// Which way a column sorts the rows of a grid, as ARIA's `aria-sort` says it: said only by the header of the column the rows are sorted by.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum SortDirection {
    Ascending,
    Descending,
}

impl SortDirection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ascending => "ascending",
            Self::Descending => "descending",
        }
    }

    /// The other way round: what a second press on the header of the sorted column asks for.
    pub fn reversed(self) -> Self {
        match self {
            Self::Ascending => Self::Descending,
            Self::Descending => Self::Ascending,
        }
    }
}
