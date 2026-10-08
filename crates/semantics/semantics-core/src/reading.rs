use std::hash::{Hash, Hasher};

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
