/// Where an item sits among the items around it: how deep in a hierarchy, and which of how many siblings, each counted from one as ARIA's `aria-level`, `aria-posinset` and `aria-setsize` count.
///
/// Said by the item itself because a virtualised tree builds only the rows on screen, so a reader counting the rows it can see would get both numbers wrong.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct SetPosition {
    pub level: u32,
    pub position: u32,
    pub size: u32,
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
