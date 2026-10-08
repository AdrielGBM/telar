/// Where an item sits among the items around it: how deep in a hierarchy, and which of how many siblings, each counted from one as ARIA's `aria-level`, `aria-posinset` and `aria-setsize` count.
///
/// Said by the item itself because a virtualised tree builds only the rows on screen, so a reader counting the rows it can see would get both numbers wrong.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct SetPosition {
    pub level: u32,
    pub position: u32,
    pub size: u32,
}
