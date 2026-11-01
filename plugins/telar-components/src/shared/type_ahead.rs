//! Type-ahead: the letters typed at a list or a tree, and the row they name.

use std::time::Duration;
use web_time::Instant;

/// How long a pause ends a query: past it the next character starts a fresh search, the same second every native list allows.
const TIMEOUT: Duration = Duration::from_millis(1000);

/// The query typed so far and when it was last typed into.
#[derive(Default)]
pub(crate) struct TypeAhead {
    query: String,
    typed_at: Option<Instant>,
}

/// What one keystroke asks for, taken out of the [`TypeAhead`] so the rows can be read without holding it: reading a label can flush effects that reach the query again.
pub(crate) struct Needle {
    text: String,
    skip_current: bool,
}

impl TypeAhead {
    /// Whether a query is still running, which is what makes a space a character rather than a key.
    pub(crate) fn is_searching(&self) -> bool {
        !self.query.is_empty() && self.typed_at.is_some_and(|t| t.elapsed() < TIMEOUT)
    }

    /// Appends `c`, starting a new query if the last keystroke has gone stale.
    ///
    /// A repeated character cycles: `d`, `d`, `d` walks the rows starting with *d* rather than searching for "ddd", which is the only way to reach the second of two rows sharing a first letter. A refined query holds still: `de` after `d` may keep the row `d` landed on, so only a one-character needle skips the current row.
    pub(crate) fn extend(&mut self, c: char) -> Needle {
        if self.typed_at.is_none_or(|t| t.elapsed() >= TIMEOUT) {
            self.query.clear();
        }
        self.query.extend(c.to_lowercase());
        self.typed_at = Some(Instant::now());
        let first = self.query.chars().next().unwrap_or(c);
        let repeated = self.query.chars().count() > 1 && self.query.chars().all(|q| q == first);
        let text = if repeated {
            first.to_string()
        } else {
            self.query.clone()
        };
        let skip_current = text.chars().count() == 1;
        Needle { text, skip_current }
    }
}

impl Needle {
    /// The first of `count` rows from `from`, once round, whose label starts with the query; `label_of` answers `None` for a row the cursor may not stop on.
    pub(crate) fn find(
        &self,
        from: Option<usize>,
        count: usize,
        label_of: impl Fn(usize) -> Option<String>,
    ) -> Option<usize> {
        let start = from.unwrap_or(0);
        (0..count).find_map(|k| {
            let i = (start + k) % count;
            if self.skip_current && Some(i) == from {
                return None;
            }
            label_of(i)
                .is_some_and(|label| label.to_lowercase().starts_with(&self.text))
                .then_some(i)
        })
    }
}

#[cfg(test)]
#[path = "type_ahead_test.rs"]
mod tests;
