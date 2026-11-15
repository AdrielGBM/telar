//! [`RovingGroup`]: items that are one Tab stop between them and are walked with the arrows, each holding focus itself as the arrows reach it.
//!
//! A set of tabs, of radios or of swatches is one control made of several. Tab enters it once, on the item chosen, and leaves it on the next press; the arrows move focus from item to item and choose the one they reach. Unlike a toolbar, which keeps focus while a cursor walks items that never hold it, every item here is a control of its own that takes focus, so a reader follows the arrows the way it follows Tab. Only the item that is the group's stop right now is in the tab order, and the stop moves with focus: whichever item holds focus is it.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use platform_core::{ConsumedKeys, Key};

use crate::context::current_direction;
use crate::focus::{self, FocusId};
use crate::keynav::{KeyNav, KeyNavMove, key_nav_apply};

/// Which arrows walk a [`RovingGroup`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RovingAxis {
    /// Left and Right, read along the surface's writing direction: a row of tabs.
    Horizontal,
    /// Up and Down: a column of options.
    Vertical,
    /// Both pairs, Right and Down toward the last item and Left and Up toward the first, with Left and Right read along the writing direction: radios or swatches, however they wrap.
    Both,
}

impl RovingAxis {
    fn keys(self) -> ConsumedKeys {
        let arrows = match self {
            Self::Horizontal => ConsumedKeys::HORIZONTAL_ARROWS,
            Self::Vertical => ConsumedKeys::VERTICAL_ARROWS,
            Self::Both => ConsumedKeys::ARROWS,
        };
        arrows.with(ConsumedKeys::EDGES)
    }
}

/// Items that are one stop in the tab order between them: Tab lands on the item chosen, or the first while none is, and the next Tab leaves the group; the arrows, Home and End move focus along the items, wrapping at either end, and choose the item they reach.
///
/// Each item is a control that joins with [`StyledContainer::roving`](crate::StyledContainer::roving) at its place in the group. A press or a reader moving to an item gives it focus directly, and the item holding focus is the group's stop, so Tab leaves from wherever focus is.
#[derive(Clone)]
pub struct RovingGroup(Rc<Group>);

struct Group {
    axis: RovingAxis,
    items: RefCell<Vec<Option<FocusId>>>,
    chosen: Box<dyn Fn() -> Option<usize>>,
    choose: Box<dyn Fn(usize)>,
}

impl RovingGroup {
    /// A group walked by the arrows of `axis`. `chosen` reads which item is chosen now, `None` while none is, and is where Tab lands; `choose` is what reaching an item with the keys does besides giving it focus, such as selecting a tab or checking a radio.
    pub fn new(
        axis: RovingAxis,
        chosen: impl Fn() -> Option<usize> + 'static,
        choose: impl Fn(usize) + 'static,
    ) -> Self {
        Self(Rc::new(Group {
            axis,
            items: RefCell::new(Vec::new()),
            chosen: Box::new(chosen),
            choose: Box::new(choose),
        }))
    }

    /// Makes the control `id` the item at `index`, in place of one built there before.
    pub(crate) fn join(&self, index: usize, id: FocusId) {
        {
            let mut items = self.0.items.borrow_mut();
            if items.len() <= index {
                items.resize(index + 1, None);
            }
            items[index] = Some(id);
        }
        // Weak: the group's closures own what the items are built from, and the focus registry outlives any one group.
        let group = Rc::downgrade(&self.0);
        focus::rove(id, move || {
            Weak::upgrade(&group).is_some_and(|group| group.stop() == Some(id))
        });
    }

    /// Takes the control `id` out of the place at `index`, unless another has been built there since.
    pub(crate) fn leave(&self, index: usize, id: FocusId) {
        if let Some(item) = self.0.items.borrow_mut().get_mut(index)
            && *item == Some(id)
        {
            *item = None;
        }
    }

    /// The keys an item keeps for the group: the arrows of its axis, Home and End.
    pub(crate) fn keys(&self) -> ConsumedKeys {
        self.0.axis.keys()
    }

    /// Moves focus from the item at `at` to the one `key` asks for and chooses it, reporting whether `key` is one the group walks with.
    pub(crate) fn key(&self, at: usize, key: &Key) -> bool {
        let Some(movement) = self.0.movement(key) else {
            return false;
        };
        let count = self.0.items.borrow().len();
        if count == 0 {
            return false;
        }
        let target = key_nav_apply(at, count, movement);
        (self.0.choose)(target);
        // Read after choosing, which may have built the item again under a new id.
        let reached = self.0.items.borrow().get(target).copied().flatten();
        if let Some(id) = reached {
            focus::request(id);
        }
        true
    }
}

impl Group {
    /// The item Tab stops at: the one holding focus, else the one chosen, else the first.
    fn stop(&self) -> Option<FocusId> {
        let items = self.items.borrow().clone();
        if let Some(held) = focus::current()
            && items.contains(&Some(held))
        {
            return Some(held);
        }
        (self.chosen)()
            .and_then(|at| items.get(at).copied().flatten())
            .or_else(|| items.iter().find_map(|item| *item))
    }

    fn movement(&self, key: &Key) -> Option<KeyNavMove> {
        let along = |nav: KeyNav| {
            nav.interpret(key).filter(|movement| {
                matches!(
                    movement,
                    KeyNavMove::Next | KeyNavMove::Previous | KeyNavMove::First | KeyNavMove::Last
                )
            })
        };
        let across = || along(KeyNav::default().horizontal().reading(current_direction()));
        let down = || along(KeyNav::default());
        match self.axis {
            RovingAxis::Horizontal => across(),
            RovingAxis::Vertical => down(),
            RovingAxis::Both => across().or_else(down),
        }
    }
}

#[cfg(test)]
#[path = "roving.test.rs"]
mod tests;
