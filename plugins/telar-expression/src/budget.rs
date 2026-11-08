//! [`Budget`]: the memory one evaluation may build, which every text and list it makes draws on, and [`TextWriter`], which builds a text inside it.

use std::cell::Cell;
use std::fmt::{self, Write};

use crate::{CallError, ErrorCode, Value};

/// The bytes one evaluation may build in all: every text it makes counts its length, every list it makes its items. Going past it is an evaluation error, so an expression from a shared bundle cannot make its host allocate without bound, however it nests `replace`, `fmt` or lists.
pub const MEMORY_BUDGET: usize = 1 << 20;

/// What remains of one evaluation's [`MEMORY_BUDGET`]. A function's implementation reaches it through [`crate::Arguments::budget`] and draws on it before it allocates.
#[derive(Debug)]
pub struct Budget {
    remaining: Cell<usize>,
}

impl Budget {
    /// A whole [`MEMORY_BUDGET`], as each evaluation starts with.
    pub fn new() -> Self {
        Self {
            remaining: Cell::new(MEMORY_BUDGET),
        }
    }

    /// Takes `bytes` from what remains, or refuses with [`ErrorCode::TooMuchMemory`] and takes nothing.
    pub fn charge(&self, bytes: usize) -> Result<(), CallError> {
        let remaining = self
            .remaining
            .get()
            .checked_sub(bytes)
            .ok_or_else(exhausted)?;
        self.remaining.set(remaining);
        Ok(())
    }

    /// `text` copied into a new value, charged before it is copied.
    pub fn text(&self, text: &str) -> Result<Value, CallError> {
        self.charge(text.len())?;
        Ok(Value::text(text))
    }

    /// An empty text to build piece by piece, each piece charged before it is appended.
    pub fn writer(&self) -> TextWriter<'_> {
        TextWriter {
            budget: self,
            out: String::new(),
        }
    }
}

impl Default for Budget {
    fn default() -> Self {
        Self::new()
    }
}

fn exhausted() -> CallError {
    CallError::new(ErrorCode::TooMuchMemory {
        budget: MEMORY_BUDGET,
    })
}

/// A text under construction that draws on a [`Budget`]: a piece that would overspend it is refused before it is written, so a list rendered into it stops at the item that crosses the limit.
#[derive(Debug)]
pub struct TextWriter<'a> {
    budget: &'a Budget,
    out: String,
}

impl TextWriter<'_> {
    pub fn push_str(&mut self, text: &str) -> Result<(), CallError> {
        self.budget.charge(text.len())?;
        self.out.push_str(text);
        Ok(())
    }

    pub fn push(&mut self, c: char) -> Result<(), CallError> {
        self.push_str(c.encode_utf8(&mut [0; 4]))
    }

    /// Appends `value` as [`Value`]'s `Display` writes it.
    pub fn push_value(&mut self, value: &Value) -> Result<(), CallError> {
        struct Bounded<'w, 'a>(&'w mut TextWriter<'a>);
        impl Write for Bounded<'_, '_> {
            fn write_str(&mut self, text: &str) -> fmt::Result {
                self.0.push_str(text).map_err(|_| fmt::Error)
            }
        }
        // Value's Display fails only when the writer refuses, so a failure here is always the budget.
        write!(Bounded(self), "{value}").map_err(|_| exhausted())
    }

    pub fn finish(self) -> Value {
        Value::text(self.out)
    }
}

#[cfg(test)]
#[path = "budget.test.rs"]
mod tests;
