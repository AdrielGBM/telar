//! [`TreeDrag`]: a tree's rows carried in a [`DragSession`], and the tree as a place where payloads from any widget of the session land, reported as a [`TreeTarget`] in the tree's own keys.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;

use telar::{
    AreaLayout, Axis, DragItem, DragSession, DropArea, DropAreaId, DropPlace, LayoutError,
    LayoutItem, Memo, NodeId, Rect, RwSignal, StyledContainer, track_layout,
};

use super::tree_view::{Flat, Row, TreeNode};
use crate::shared::drag_label::drag_label;
use crate::shared::rows::{RowKey, RowPlaces};

/// Where in a tree a payload lands, named by the tree's own keys rather than by the rows it happens to show.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TreeTarget<K> {
    /// Onto the node `K`: into a branch, where among what it holds being the app's to say, or onto a leaf that takes what is dropped on it.
    Inside(K),
    /// Into the gap among the nodes under `parent` (`None` for the top level), right after the node `after` and right before the node `before`, each `None` at that end of them.
    Between {
        parent: Option<K>,
        after: Option<K>,
        before: Option<K>,
    },
}

/// What a tree hands its [`TreeDrag`] as it is built: the rows it shows, the branches it has open, and where the rows stand.
pub(super) struct TreeRows<K: RowKey> {
    pub(super) flat: Memo<Flat<K>>,
    pub(super) expanded: RwSignal<HashSet<K>>,
    pub(super) places: RowPlaces,
}

/// A tree's part in a drag, as the tree uses it once built, whatever the session carries.
pub(super) trait Dragging<K> {
    /// `row`, the box of the row showing `node`, made an item of the session when the node carries anything.
    fn item(self: Rc<Self>, row: StyledContainer, node: &TreeNode<K>) -> StyledContainer;
    /// Registers `list`, the box the rows are laid out in, as an area of the session, until the tree goes.
    fn register(self: Rc<Self>, list: NodeId);
    /// Where among the rows shown the payload in hand would land, reactively.
    fn place(&self) -> Option<DropPlace<usize>>;
    /// What a node of the tree in hand is called, drawn beside the pointer.
    fn label(self: Rc<Self>) -> Result<Box<dyn LayoutItem>, LayoutError>;
}

type Bind<K> = Rc<dyn Fn(TreeRows<K>) -> Rc<dyn Dragging<K>>>;

/// How a [`tree_view`](super::tree_view::tree_view)'s rows take part in a [`DragSession`]: carried between their siblings and into branches, and taking payloads any other widget of the session carries.
///
/// A row is carried as what `carries` makes of its node, and not at all where it makes nothing. Wherever a payload would land, the tree asks `accepts` with the [`TreeTarget`] it names, so what a node takes is the app's to say from its own types: a folder taking a text into it, a section of a text, a leaf of the tree, taking a sentence onto it. The tree itself refuses only a node of its own into itself or anything under it. A drop is told to `on_drop`, except a node put back in the gap it came out of. Held over a shut branch, the pointer opens it.
pub struct TreeDrag<K: RowKey>(Bind<K>);

impl<K: RowKey> Clone for TreeDrag<K> {
    fn clone(&self) -> Self {
        Self(Rc::clone(&self.0))
    }
}

impl<K: RowKey> TreeDrag<K> {
    pub fn new<P: Clone + 'static>(
        session: &DragSession<P>,
        carries: impl Fn(&TreeNode<K>) -> Option<P> + 'static,
        accepts: impl Fn(&P, &TreeTarget<K>) -> bool + 'static,
        on_drop: impl Fn(P, TreeTarget<K>) + 'static,
    ) -> Self {
        let session = session.clone();
        let rules = Rc::new(Rules {
            carries: Box::new(carries),
            accepts: Box::new(accepts),
            on_drop: Box::new(on_drop),
        });
        Self(Rc::new(move |rows| {
            Rc::new(Bound {
                session: session.clone(),
                rules: Rc::clone(&rules),
                rows,
                area: Cell::new(None),
                picked: RefCell::new(None),
            }) as Rc<dyn Dragging<K>>
        }))
    }

    pub(super) fn bind(&self, rows: TreeRows<K>) -> Rc<dyn Dragging<K>> {
        (self.0)(rows)
    }
}

type Carries<K, P> = Box<dyn Fn(&TreeNode<K>) -> Option<P>>;
type Accepts<K, P> = Box<dyn Fn(&P, &TreeTarget<K>) -> bool>;
type OnDrop<K, P> = Box<dyn Fn(P, TreeTarget<K>)>;

struct Rules<K, P> {
    carries: Carries<K, P>,
    accepts: Accepts<K, P>,
    on_drop: OnDrop<K, P>,
}

/// A [`TreeDrag`] bound to one tree.
struct Bound<K: RowKey, P: Clone + 'static> {
    session: DragSession<P>,
    rules: Rc<Rules<K, P>>,
    rows: TreeRows<K>,
    area: Cell<Option<DropAreaId>>,
    /// The row of the tree last picked up: its box, as the session names what it carries, and its node's key, which outlives the box once the row scrolls out of the rows built.
    picked: RefCell<Option<(NodeId, K)>>,
}

impl<K: RowKey, P: Clone + 'static> Bound<K, P> {
    fn shown(&self) -> Rc<Vec<Rc<Row<K>>>> {
        self.rows.flat.with(|flat| Rc::clone(&flat.0))
    }

    /// The node of this tree in hand, reactively.
    fn carried(&self) -> Option<K> {
        let from = self.session.carried_from()?;
        self.picked
            .borrow()
            .as_ref()
            .filter(|(node, _)| *node == from)
            .map(|(_, key)| key.clone())
    }

    fn layout(&self, bounds: Rect) -> AreaLayout {
        let count = self.rows.flat.with(|flat| flat.0.len());
        let items = (0..count)
            .map(|index| {
                let (top, height) = self.rows.places.span(index);
                Rect::new(bounds.x, bounds.y + top, bounds.width, height)
            })
            .collect();
        AreaLayout { bounds, items }
    }

    fn admits(&self, payload: &P, place: DropPlace<usize>) -> bool {
        let rows = self.shown();
        let Some(target) = target_of(&rows, place) else {
            return false;
        };
        if let Some(carried) = self.carried()
            && lands_in_itself(&rows, &carried, &target)
        {
            return false;
        }
        (self.rules.accepts)(payload, &target)
    }

    fn land(&self, payload: P, place: DropPlace<usize>) {
        let Some(target) = target_of(&self.shown(), place) else {
            return;
        };
        if let Some(carried) = self.carried()
            && puts_back(&carried, &target)
        {
            return;
        }
        (self.rules.on_drop)(payload, target);
    }

    /// Opens the shut branch at row `index`, held over: unless it is the node in hand, or under it.
    fn open_held(&self, index: usize) {
        let rows = self.shown();
        let Some(row) = rows.get(index).filter(|row| row.node.is_branch()) else {
            return;
        };
        let id = row.node.id.clone();
        if let Some(carried) = self.carried()
            && subtree(&rows, &carried).contains(&id)
        {
            return;
        }
        if !self.rows.expanded.with(|open| open.contains(&id)) {
            self.rows.expanded.update(|open| {
                open.insert(id);
            });
        }
    }
}

impl<K: RowKey, P: Clone + 'static> Dragging<K> for Bound<K, P> {
    fn item(self: Rc<Self>, row: StyledContainer, node: &TreeNode<K>) -> StyledContainer {
        if (self.rules.carries)(node).is_none() {
            return row;
        }
        let from = row.layout_node();
        let label = node.label.clone();
        let item = DragItem::new({
            let (bound, node) = (Rc::clone(&self), node.clone());
            move || {
                let payload = (bound.rules.carries)(&node)?;
                *bound.picked.borrow_mut() = Some((from, node.id.clone()));
                Some(payload)
            }
        })
        .label(move || label.to_string());
        self.session.item(row, item)
    }

    fn register(self: Rc<Self>, list: NodeId) {
        let Some(laid) = track_layout(list) else {
            return;
        };
        let accepts = {
            let bound = Rc::clone(&self);
            move |payload: &P, place| bound.admits(payload, place)
        };
        let on_drop = {
            let bound = Rc::clone(&self);
            move |payload: P, place| bound.land(payload, place)
        };
        let layout = {
            let bound = Rc::clone(&self);
            move || bound.layout(laid.get())
        };
        let on_hold = {
            let bound = Rc::clone(&self);
            move |_: &P, index| bound.open_held(index)
        };
        let area = self.session.area(
            DropArea::new(list, Axis::Vertical, accepts, on_drop)
                .layout(layout)
                .on_hold(on_hold),
        );
        self.area.set(Some(area));
    }

    fn place(&self) -> Option<DropPlace<usize>> {
        self.session.place_in(self.area.get()?)
    }

    fn label(self: Rc<Self>) -> Result<Box<dyn LayoutItem>, LayoutError> {
        let bound = Rc::clone(&self);
        drag_label(&self.session, move || bound.carried().is_some())
    }
}

/// The place `place` among `rows` names, in the tree's keys: onto a row is into its node, and the gap before a row is among that row's siblings, after the one before it. The gap after the last row is at the end of the top level.
pub(super) fn target_of<K: RowKey>(
    rows: &[Rc<Row<K>>],
    place: DropPlace<usize>,
) -> Option<TreeTarget<K>> {
    let index = match place {
        DropPlace::Inside(index) => return Some(TreeTarget::Inside(rows.get(index)?.node.id.clone())),
        DropPlace::Between(index) => index,
    };
    let Some(row) = rows.get(index) else {
        let after = rows.iter().rev().find(|row| row.parent.is_none());
        return Some(TreeTarget::Between {
            parent: None,
            after: after.map(|row| row.node.id.clone()),
            before: None,
        });
    };
    let after = rows[..index]
        .iter()
        .rev()
        .find(|earlier| earlier.parent == row.parent);
    Some(TreeTarget::Between {
        parent: row.parent.map(|parent| rows[parent].node.id.clone()),
        after: after.map(|row| row.node.id.clone()),
        before: Some(row.node.id.clone()),
    })
}

/// `node` and every node shown under it.
fn subtree<K: RowKey>(rows: &[Rc<Row<K>>], node: &K) -> Vec<K> {
    let Some(at) = rows.iter().position(|row| row.node.id == *node) else {
        return vec![node.clone()];
    };
    let level = rows[at].position.level;
    std::iter::once(&rows[at])
        .chain(rows[at + 1..].iter().take_while(|row| row.position.level > level))
        .map(|row| row.node.id.clone())
        .collect()
}

/// Whether `target` puts the node `carried` into itself or into something under it.
fn lands_in_itself<K: RowKey>(rows: &[Rc<Row<K>>], carried: &K, target: &TreeTarget<K>) -> bool {
    let into = match target {
        TreeTarget::Inside(node) => node,
        TreeTarget::Between {
            parent: Some(parent),
            ..
        } => parent,
        TreeTarget::Between { parent: None, .. } => return false,
    };
    subtree(rows, carried).contains(into)
}

/// Whether `target` is the gap the node `carried` stands in already, on either side of it.
fn puts_back<K: RowKey>(carried: &K, target: &TreeTarget<K>) -> bool {
    matches!(target, TreeTarget::Between { after, before, .. }
        if after.as_ref() == Some(carried) || before.as_ref() == Some(carried))
}

#[cfg(test)]
#[path = "tree_drag.test.rs"]
mod tests;
