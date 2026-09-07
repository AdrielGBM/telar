//! Moving a selection through a list with the keyboard, the same way in every list that has one.
//!
//! It is deliberately *not* the focus system. That answers "which widget receives keys" — one focusable per widget, driven by Tab. This answers "which row of a list is selected", which is one focusable holding a cursor over N rows, and the two compose: a search field keeps focus while these keys drive the list underneath it.
//!
//! The important half of the contract is the negative one: **everything [`KeyNav::interpret`] returns `None` for must still reach a focused text field as typing.** A list that swallows `j` cannot also be searched, which is why the vim bindings are off unless a caller asks for them.

use platform_core::{Key, NamedKey};

/// A default vertical list: arrows, Home/End, Enter and Escape, and no vim bindings.
impl Default for KeyNav {
    fn default() -> Self {
        Self {
            vim: false,
            horizontal: false,
            grid: false,
        }
    }
}

/// What a key press means to a list.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KeyNavMove {
    Next,
    Previous,
    First,
    Last,
    /// One row down in a grid — a whole column count, not one tile. Same as [`KeyNavMove::Next`] in a single-column list, which is what lets one `apply` serve both.
    NextRow,
    PreviousRow,
    /// Run the selected row.
    Activate,
    /// Back out: dismiss the surface, or undo an armed confirmation.
    Cancel,
}

/// How a list reads keys: the arrows always, and optionally the vim bindings on top.
///
/// `vim` is off by default because a list that swallows `j` cannot also be typed into, and hyprshell's biggest list — the launcher — is a search field. A surface with no text input can turn it on freely; one with a field should only do so if its user asked for it.
#[derive(Clone, Copy)]
pub struct KeyNav {
    pub vim: bool,
    /// The list runs along the screen's horizontal, so Left/Right move it rather than Up/Down.
    pub horizontal: bool,
    /// The list wraps into rows, so it uses *both* pairs of arrows: Left/Right for one tile and Up/Down for a whole row. Only a grid can, which is why it is a mode rather than the default.
    pub grid: bool,
}

impl KeyNav {
    pub fn horizontal(mut self) -> Self {
        self.horizontal = true;
        self
    }

    /// A grid: tiles run along a row, so Left/Right step one and Up/Down step a row.
    pub fn grid(mut self) -> Self {
        self.horizontal = true;
        self.grid = true;
        self
    }

    /// What `key` asks the list to do, or `None` when it is not a navigation key — which is the important half of the contract: everything this returns `None` for must still reach a focused text field as typing.
    pub fn interpret(self, key: &Key) -> Option<KeyNavMove> {
        let (forward, back) = if self.horizontal {
            (NamedKey::ArrowRight, NamedKey::ArrowLeft)
        } else {
            (NamedKey::ArrowDown, NamedKey::ArrowUp)
        };
        if let Key::Named(named) = key {
            if *named == forward {
                return Some(KeyNavMove::Next);
            }
            if *named == back {
                return Some(KeyNavMove::Previous);
            }
            if self.grid {
                if *named == NamedKey::ArrowDown {
                    return Some(KeyNavMove::NextRow);
                }
                if *named == NamedKey::ArrowUp {
                    return Some(KeyNavMove::PreviousRow);
                }
            }
            return match named {
                NamedKey::Enter => Some(KeyNavMove::Activate),
                NamedKey::Escape => Some(KeyNavMove::Cancel),
                NamedKey::Home => Some(KeyNavMove::First),
                NamedKey::End => Some(KeyNavMove::Last),
                _ => None,
            };
        }
        if !self.vim {
            return None;
        }
        // Vim's own pairs plus the readline pair. `G` before `g`, since the shift distinction is their whole difference.
        let (down, up) = if self.grid {
            (KeyNavMove::NextRow, KeyNavMove::PreviousRow)
        } else {
            (KeyNavMove::Next, KeyNavMove::Previous)
        };
        match key {
            Key::Char(c) => match c {
                'j' => Some(down),
                'k' => Some(up),
                'h' if self.grid => Some(KeyNavMove::Previous),
                'l' if self.grid => Some(KeyNavMove::Next),
                'g' => Some(KeyNavMove::First),
                'G' => Some(KeyNavMove::Last),
                '\u{e}' => Some(down), // Ctrl-N
                '\u{10}' => Some(up),  // Ctrl-P
                _ => None,
            },
            _ => None,
        }
    }
}

/// Where a move lands, given the current index and how many rows there are.
///
/// Wraps at both ends: a list short enough to see all of is faster to reach the bottom of by pressing up once, and a list too long to see wraps rather than sticking silently, which reads as the key not working.
pub fn key_nav_apply(current: usize, count: usize, movement: KeyNavMove) -> usize {
    key_nav_apply_grid(current, count, 1, movement)
}

/// Where a move lands in a grid `columns` tiles wide. A single column is a list, which is why [`key_nav_apply`] is this with `columns = 1` rather than a second implementation.
///
/// A row move off the bottom lands on the nearest tile below where there is one — a partial last row is still a row — and wraps to the same column otherwise, matching the horizontal rule.
pub fn key_nav_apply_grid(
    current: usize,
    count: usize,
    columns: usize,
    movement: KeyNavMove,
) -> usize {
    if count == 0 {
        return 0;
    }
    let columns = columns.max(1);
    let current = current.min(count - 1);
    let last = count - 1;
    match movement {
        KeyNavMove::Next => (current + 1) % count,
        KeyNavMove::Previous => (current + count - 1) % count,
        KeyNavMove::First => 0,
        KeyNavMove::Last => last,
        KeyNavMove::NextRow => {
            let below = current + columns;
            if below <= last {
                below
            } else if current / columns < last / columns {
                // A shorter last row: down from the end of a full row still goes down, to its final tile.
                last
            } else {
                current % columns
            }
        }
        KeyNavMove::PreviousRow => {
            if current >= columns {
                current - columns
            } else {
                let bottom = (last / columns) * columns + current % columns;
                if bottom > last {
                    bottom - columns
                } else {
                    bottom
                }
            }
        }
        KeyNavMove::Activate | KeyNavMove::Cancel => current,
    }
}

#[cfg(test)]
#[path = "keynav_test.rs"]
mod tests;
