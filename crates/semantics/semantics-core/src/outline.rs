use std::num::NonZeroU32;

/// How deep a heading sits in the outline of its screen: one of six levels, [`One`](Self::One) the screen's own title.
///
/// A closed set rather than a number, so a level no reader has, such as zero or seven, cannot be said and every target announces the same one.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum HeadingLevel {
    One,
    Two,
    Three,
    Four,
    Five,
    Six,
}

impl HeadingLevel {
    /// Every level, outermost first.
    pub const ALL: [HeadingLevel; 6] = [
        Self::One,
        Self::Two,
        Self::Three,
        Self::Four,
        Self::Five,
        Self::Six,
    ];

    /// The level `level` counts, from one, or `None` for a number outside one to six.
    pub const fn new(level: u8) -> Option<Self> {
        match level {
            1 => Some(Self::One),
            2 => Some(Self::Two),
            3 => Some(Self::Three),
            4 => Some(Self::Four),
            5 => Some(Self::Five),
            6 => Some(Self::Six),
            _ => None,
        }
    }

    /// The level of the outline nearest `level`: one below it, six above it.
    pub const fn nearest(level: u8) -> Self {
        match Self::new(level) {
            Some(level) => level,
            None if level == 0 => Self::One,
            None => Self::Six,
        }
    }

    /// The level counted from one, as a reader announces it.
    pub const fn get(self) -> u8 {
        match self {
            Self::One => 1,
            Self::Two => 2,
            Self::Three => 3,
            Self::Four => 4,
            Self::Five => 5,
            Self::Six => 6,
        }
    }
}

/// Where an item sits among the items around it: how deep in a hierarchy, and which of how many siblings, each counted from one as ARIA's `aria-level`, `aria-posinset` and `aria-setsize` count.
///
/// Said by the item itself because a virtualised tree builds only the rows on screen, so a reader counting the rows it can see would get both numbers wrong.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct SetPosition {
    pub level: NonZeroU32,
    pub position: NonZeroU32,
    pub size: NonZeroU32,
}

/// Which of a [`SetPosition`]'s numbers an item of a role can say, as ARIA allows them on it: a number a role does not take is one a reader rejects, or one a checker flags.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Positioning {
    /// Its level and its place in its set: a row of a tree, an item of a list.
    Nested,
    /// Its place in its set and nothing about a level: an option, a row of a menu, a tab, a radio.
    InSet,
    /// Its place among the rows of the grid it is in, which the grid counts: its [`position`](SetPosition::position) is its row's index and its [`size`](SetPosition::size) the grid's count of rows, the header's included.
    GridRow,
}

#[cfg(test)]
#[path = "outline.test.rs"]
mod tests;
