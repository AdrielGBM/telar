//! [`SelectionMode`], and what a list or a table that walks its rows with a cursor keeps of the rows chosen: one row, or any number of them and the row a Shift range runs from.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;
use std::sync::Arc;

use telar::{ModifiersState, RwSignal, signal};

/// How many rows of a [`virtual_list`](crate::virtual_list) or a [`data_table`](crate::data_table) can be chosen at once.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, telar::PreviewArg)]
pub enum SelectionMode {
    /// One: a press, Enter or Space chooses the row in place of the one chosen before.
    #[default]
    Single,
    /// Any number. A press or Enter chooses the row alone; Ctrl with a press (Command on a Mac) or Space adds the row or takes it away; Shift with a press, an arrow, Home, End, Page Up, Page Down or Space chooses every row from the one last pressed, toggled or walked to without Shift; Ctrl+A (Command+A) chooses every row, or none once all are.
    Multiple,
}

/// What a list or a table calls with the ids of the rows chosen each time a press or a key changes them, in [`SelectionMode::Multiple`].
pub type SelectionHandler = Rc<dyn Fn(&BTreeSet<Arc<str>>)>;

/// What a press asks of the selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Gesture {
    /// The row alone.
    Only,
    /// The row in or out, the others kept.
    Toggle,
    /// Every row from the anchor to this one: in place of the rows chosen, or beside them when `keep`.
    Range { keep: bool },
}

impl Gesture {
    /// What a press with `modifiers` held asks for in `mode`.
    pub(crate) fn of_press(mode: SelectionMode, modifiers: ModifiersState) -> Self {
        match (mode, modifiers.is_shift, modifiers.is_primary()) {
            (SelectionMode::Single, ..) => Self::Only,
            (SelectionMode::Multiple, true, keep) => Self::Range { keep },
            (SelectionMode::Multiple, false, true) => Self::Toggle,
            (SelectionMode::Multiple, false, false) => Self::Only,
        }
    }
}

/// The rows chosen, by id, and in [`SelectionMode::Multiple`] the row a Shift range runs from.
pub(crate) struct RowSelection {
    mode: SelectionMode,
    one: RwSignal<Option<Arc<str>>>,
    many: RwSignal<BTreeSet<Arc<str>>>,
    anchor: RefCell<Option<Arc<str>>>,
    on_change: Option<SelectionHandler>,
}

impl RowSelection {
    /// A selection in `mode`, kept in `one` while it is [`Single`](SelectionMode::Single) and in `many` while it is [`Multiple`](SelectionMode::Multiple), each a signal of its own where the caller bound none.
    pub(crate) fn new(
        mode: SelectionMode,
        one: Option<RwSignal<Option<Arc<str>>>>,
        many: Option<RwSignal<BTreeSet<Arc<str>>>>,
        on_change: Option<SelectionHandler>,
    ) -> Self {
        Self {
            mode,
            one: one.unwrap_or_else(|| signal(None)),
            many: many.unwrap_or_else(|| signal(BTreeSet::new())),
            anchor: RefCell::default(),
            on_change,
        }
    }

    pub(crate) fn mode(&self) -> SelectionMode {
        self.mode
    }

    pub(crate) fn is_multiple(&self) -> bool {
        self.mode == SelectionMode::Multiple
    }

    /// Whether the row `id` is chosen, subscribing the caller to the selection.
    pub(crate) fn is_selected(&self, id: &str) -> bool {
        match self.mode {
            SelectionMode::Single => self.one.with(|one| one.as_deref() == Some(id)),
            SelectionMode::Multiple => self.many.with(|many| many.contains(id)),
        }
    }

    /// The first of `ids` that is chosen, read without subscribing: where a cursor arriving at the rows starts.
    pub(crate) fn first_of<'a>(
        &self,
        mut ids: impl Iterator<Item = &'a Arc<str>>,
    ) -> Option<Arc<str>> {
        match self.mode {
            SelectionMode::Single => {
                let one = self.one.peek()?;
                ids.find(|id| **id == one).cloned()
            }
            SelectionMode::Multiple => self
                .many
                .peek_with(|many| ids.find(|id| many.contains(&***id)).cloned()),
        }
    }

    /// Makes `id` the row a Shift range runs from.
    pub(crate) fn anchor_at(&self, id: &Arc<str>) {
        *self.anchor.borrow_mut() = Some(id.clone());
    }

    /// Does what `gesture` asks of the row `id`, among the rows of `order` that can be chosen, in the order shown. A range with no anchor among them runs from `from`, the row the cursor was on.
    pub(crate) fn apply(
        &self,
        gesture: Gesture,
        id: &Arc<str>,
        order: impl FnOnce() -> Vec<Arc<str>>,
        from: Option<&Arc<str>>,
    ) {
        match gesture {
            Gesture::Only => self.only(id),
            Gesture::Toggle => self.toggle(id),
            Gesture::Range { keep } => self.range(id, &order(), keep, from),
        }
    }

    pub(crate) fn only(&self, id: &Arc<str>) {
        self.anchor_at(id);
        match self.mode {
            SelectionMode::Single => self.one.set(Some(id.clone())),
            SelectionMode::Multiple => self.change(|many| {
                many.clear();
                many.insert(id.clone());
            }),
        }
    }

    fn toggle(&self, id: &Arc<str>) {
        if !self.is_multiple() {
            return self.only(id);
        }
        self.anchor_at(id);
        self.change(|many| {
            if !many.remove(&**id) {
                many.insert(id.clone());
            }
        });
    }

    fn range(&self, id: &Arc<str>, order: &[Arc<str>], keep: bool, from: Option<&Arc<str>>) {
        if !self.is_multiple() {
            return self.only(id);
        }
        let at = |id: &Arc<str>| order.iter().position(|row| row == id);
        let Some(end) = at(id) else {
            return;
        };
        let anchored = self.anchor.borrow().as_ref().and_then(at);
        let start = anchored.or_else(|| from.and_then(at)).unwrap_or(end);
        if anchored.is_none() {
            self.anchor_at(&order[start]);
        }
        let run = &order[start.min(end)..=start.max(end)];
        self.change(|many| {
            if !keep {
                many.clear();
            }
            many.extend(run.iter().cloned());
        });
    }

    /// Chooses every row of `order`, or none of them once all are chosen. Nothing in [`SelectionMode::Single`].
    pub(crate) fn all(&self, order: &[Arc<str>]) {
        if !self.is_multiple() {
            return;
        }
        self.change(|many| {
            if order.iter().all(|id| many.contains(&**id)) {
                for id in order {
                    many.remove(&**id);
                }
            } else {
                many.extend(order.iter().cloned());
            }
        });
    }

    fn change(&self, edit: impl FnOnce(&mut BTreeSet<Arc<str>>)) {
        let mut next = self.many.peek();
        edit(&mut next);
        if self.many.peek_with(|now| *now == next) {
            return;
        }
        self.many.set(next.clone());
        if let Some(on_change) = &self.on_change {
            on_change(&next);
        }
    }
}

#[cfg(test)]
#[path = "selection_test.rs"]
mod tests;
