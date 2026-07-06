use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use geometry_core::Rect;
use layout_core::{LayoutError, LayoutStyle, NodeId};
use platform_core::Event;
use reactive_core::{Effect, RwSignal, effect, signal};
use ui_tree::{Component, EventResult, RenderNode};

use crate::context::{WidgetCtx, new_container, remove_node, set_children, track_layout};
use crate::layout_item::{Child, LayoutItem, TrackedChildren, make_child};
use crate::pointer::dispatch_container_event;

/// A key erased to a `u64` so the list state stays non-generic. A collision would reuse the wrong item's
/// node, but a 64-bit hash makes that astronomically unlikely for the small, distinct keys a list uses.
fn hash_key<K: Hash>(k: &K) -> u64 {
    let mut h = DefaultHasher::new();
    k.hash(&mut h);
    h.finish()
}

/// Shared reconciliation state: the container node plus the current items in order and their key hashes.
/// Mutated by the reconcile effect (during the reactive flush) and read by `view`/`on_event` (during
/// render/dispatch) — never concurrently, so the `RefCell` never double-borrows.
struct ListState {
    node: NodeId,
    children: TrackedChildren,
    keys: Vec<u64>,
}

/// A reactive list: `for item in $items key id` (or, keyless, `for item in $items`) in `.rsx`. Re-runs its
/// source reactively and reconciles the item widgets — reused keys/positions keep their node/widget, new
/// ones are built, gone ones are disposed, and the layout children are reordered — instead of rebuilding the
/// whole block on every change. `new`/`with_gap` reconcile by key (identity-stable); `positional`/
/// `positional_with_gap` reconcile by index (no `key` clause needed, cheap append/truncate).
pub struct ReactiveList {
    node: NodeId,
    rect: RwSignal<Rect>,
    state: Rc<RefCell<ListState>>,
    // Bumped on every reconcile so `view()` (which reads it) re-emits the new/reordered child group.
    version: RwSignal<u64>,
    // Keeps the reconcile effect alive for the widget's lifetime.
    _effect: Effect,
}

impl ReactiveList {
    /// `source` reads the reactive item collection; `key` extracts a stable identity per item; `build`
    /// constructs one widget per item. `build` takes a `&mut WidgetCtx` handle so it can create nodes
    /// against the live (thread-local) layout tree from inside the reconcile effect.
    pub fn new<Item, Key, S, K, B>(
        ctx: &mut WidgetCtx,
        source: S,
        key: K,
        build: B,
    ) -> Result<Self, LayoutError>
    where
        Key: Hash + 'static,
        Item: 'static,
        S: Fn() -> Vec<Item> + 'static,
        K: Fn(&Item) -> Key + 'static,
        B: Fn(&mut WidgetCtx, Item) -> Result<Box<dyn LayoutItem>, LayoutError> + 'static,
    {
        Self::build(ctx, source, build, 0.0, move |item: &Item, _idx: usize| {
            hash_key(&key(item))
        })
    }

    /// Same as `new`, but with a `gap` (px) laid out between item containers — `for … key … gap:N` in `.rsx`.
    pub fn with_gap<Item, Key, S, K, B>(
        ctx: &mut WidgetCtx,
        source: S,
        key: K,
        build: B,
        gap: f32,
    ) -> Result<Self, LayoutError>
    where
        Key: Hash + 'static,
        Item: 'static,
        S: Fn() -> Vec<Item> + 'static,
        K: Fn(&Item) -> Key + 'static,
        B: Fn(&mut WidgetCtx, Item) -> Result<Box<dyn LayoutItem>, LayoutError> + 'static,
    {
        Self::build(ctx, source, build, gap, move |item: &Item, _idx: usize| {
            hash_key(&key(item))
        })
    }

    /// A keyless reactive list: `for item in $items` with no `key` clause. Reconciles by POSITION — the
    /// item at index `i` always reuses the node previously at index `i`, so an append/truncate reuses every
    /// surviving node cheaply, but a reorder rebuilds rather than moving nodes (no per-item identity without
    /// a key).
    pub fn positional<Item, S, B>(
        ctx: &mut WidgetCtx,
        source: S,
        build: B,
    ) -> Result<Self, LayoutError>
    where
        Item: 'static,
        S: Fn() -> Vec<Item> + 'static,
        B: Fn(&mut WidgetCtx, Item) -> Result<Box<dyn LayoutItem>, LayoutError> + 'static,
    {
        Self::build(ctx, source, build, 0.0, |_item: &Item, idx: usize| {
            idx as u64
        })
    }

    /// `positional` with an item gap — `for item in $items gap:N` (no `key`).
    pub fn positional_with_gap<Item, S, B>(
        ctx: &mut WidgetCtx,
        source: S,
        build: B,
        gap: f32,
    ) -> Result<Self, LayoutError>
    where
        Item: 'static,
        S: Fn() -> Vec<Item> + 'static,
        B: Fn(&mut WidgetCtx, Item) -> Result<Box<dyn LayoutItem>, LayoutError> + 'static,
    {
        Self::build(ctx, source, build, gap, |_item: &Item, idx: usize| {
            idx as u64
        })
    }

    /// Shared constructor: `keyer` erases both reconciliation modes (hashed key, or plain index) to a
    /// `u64` so `reconcile` doesn't need to know which mode produced it.
    fn build<Item, S, B, KeyFn>(
        ctx: &mut WidgetCtx,
        source: S,
        build: B,
        gap: f32,
        keyer: KeyFn,
    ) -> Result<Self, LayoutError>
    where
        Item: 'static,
        S: Fn() -> Vec<Item> + 'static,
        B: Fn(&mut WidgetCtx, Item) -> Result<Box<dyn LayoutItem>, LayoutError> + 'static,
        KeyFn: Fn(&Item, usize) -> u64 + 'static,
    {
        let node = new_container(ctx, LayoutStyle::new().flex_column().gap(gap), &[])?;
        let rect = track_layout(ctx, node).expect("list container is registered");
        let state = Rc::new(RefCell::new(ListState {
            node,
            children: Vec::new(),
            keys: Vec::new(),
        }));
        let version = signal(0u64);

        let eff_state = Rc::clone(&state);
        let eff_version = version.clone();
        // Runs once now (builds the initial list) and again on every change to a signal `source` reads.
        let _effect = effect(move || {
            let items = source();
            reconcile(&eff_state, items, &keyer, &build);
            eff_version.update(|v| *v = v.wrapping_add(1));
        });

        Ok(Self {
            node,
            rect,
            state,
            version,
            _effect,
        })
    }
}

fn reconcile<Item, KeyFn, B>(
    state: &Rc<RefCell<ListState>>,
    items: Vec<Item>,
    keyer: &KeyFn,
    build: &B,
) where
    KeyFn: Fn(&Item, usize) -> u64,
    B: Fn(&mut WidgetCtx, Item) -> Result<Box<dyn LayoutItem>, LayoutError>,
{
    let mut st = state.borrow_mut();
    let container = st.node;

    // Index the current children by key hash so a persisting key reuses its widget/node.
    let old_keys = std::mem::take(&mut st.keys);
    let old_children = std::mem::take(&mut st.children);
    let mut old: HashMap<u64, Child> = HashMap::new();
    for (k, child) in old_keys.into_iter().zip(old_children) {
        old.entry(k).or_insert(child);
    }

    let mut ctx = WidgetCtx::handle();
    let mut children: TrackedChildren = Vec::with_capacity(items.len());
    let mut keys: Vec<u64> = Vec::with_capacity(items.len());
    let mut nodes: Vec<NodeId> = Vec::with_capacity(items.len());

    for (idx, item) in items.into_iter().enumerate() {
        let k = keyer(&item, idx);
        let child = match old.remove(&k) {
            Some(existing) => existing,
            None => make_child(build(&mut ctx, item).expect("reactive list item build")),
        };
        nodes.push(child.node());
        children.push(child);
        keys.push(k);
    }

    st.children = children;
    st.keys = keys;
    drop(st);

    // Reorder/insert/drop in the layout tree, then free the nodes of items that went away. set_children
    // first so the disposed nodes are detached before remove_node frees them.
    let _ = set_children(container, &nodes);
    for (_, child) in old {
        remove_node(child.node());
    }
}

impl LayoutItem for ReactiveList {
    fn layout_node(&self) -> NodeId {
        self.node
    }
}

impl Component for ReactiveList {
    fn view(&self) -> RenderNode {
        // Subscribe to reconciles so the child group re-emits when items are added/removed/reordered.
        self.version.get();
        let _ = self.rect.get();
        let st = self.state.borrow();
        RenderNode::group(st.children.iter().map(|c| c.segment.boundary()))
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        let mut st = self.state.borrow_mut();
        dispatch_container_event(&mut st.children, event)
    }

    fn debug_name(&self) -> &'static str {
        "ReactiveList"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container::Container;
    use reactive_core::signal;

    fn leaf(ctx: &mut WidgetCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
        Ok(Box::new(Container::new(
            ctx,
            LayoutStyle::new().width(10.0).height(10.0),
            vec![],
        )?))
    }

    // The effect runs once at construction, so the list is populated from the initial source.
    #[test]
    fn builds_initial_items() {
        let mut ctx = WidgetCtx::new();
        let items = signal(vec![1, 2, 3]);
        let src = items.clone();
        let list =
            ReactiveList::new(&mut ctx, move || src.get(), |n: &i32| *n, |c, _| leaf(c)).unwrap();
        assert_eq!(list.state.borrow().children.len(), 3);
    }

    // A reorder-plus-remove reuses the persisting items' nodes (keyed) and drops the gone one.
    #[test]
    fn reconcile_reuses_nodes_on_reorder_and_remove() {
        let mut ctx = WidgetCtx::new();
        let items = signal(vec![1, 2, 3]);
        let src = items.clone();
        let list =
            ReactiveList::new(&mut ctx, move || src.get(), |n: &i32| *n, |c, _| leaf(c)).unwrap();
        let v1: Vec<NodeId> = list
            .state
            .borrow()
            .children
            .iter()
            .map(|c| c.node())
            .collect();
        assert_eq!(v1.len(), 3);

        // Outside a batch, `set` flushes the effect immediately → reconcile runs now.
        items.set(vec![3, 1]);

        let st = list.state.borrow();
        assert_eq!(st.children.len(), 2, "item 2 should be dropped");
        let v2: Vec<NodeId> = st.children.iter().map(|c| c.node()).collect();
        assert_eq!(v2[0], v1[2], "item 3 keeps its node, moved to front");
        assert_eq!(v2[1], v1[0], "item 1 keeps its node");
    }

    // The full runtime flow: after a signal change, the new item is reconciled AND laid out (non-zero
    // rect) once the runtime relayouts — proving relayout_if_dirty picks up a deep reactive change.
    #[test]
    fn added_item_gets_laid_out_after_relayout() {
        use crate::context::{compute_layout, relayout_if_dirty, track_layout};
        use layout_core::AvailableSpace;

        let mut ctx = WidgetCtx::new();
        let items = signal(vec![1i32, 2]);
        let src = items.clone();
        let list =
            ReactiveList::new(&mut ctx, move || src.get(), |n: &i32| *n, |c, _| leaf(c)).unwrap();
        let list_node = list.layout_node();
        compute_layout(
            &mut ctx,
            list_node,
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(200.0),
        )
        .unwrap();
        assert!(
            track_layout(&ctx, list.state.borrow().children[0].node())
                .unwrap()
                .get()
                .height
                > 0.0,
            "initial items should be laid out"
        );

        // A data change: the effect reconciles (adds item 3, dirtying the container up to the root).
        items.set(vec![1, 2, 3]);
        assert_eq!(list.state.borrow().children.len(), 3, "item added");

        // The runtime relayouts every known root (the list node among them), which picks up the new item.
        relayout_if_dirty();

        let n2 = list.state.borrow().children[2].node();
        assert!(
            track_layout(&ctx, n2).unwrap().get().height > 0.0,
            "the newly added item must be laid out after relayout_if_dirty"
        );
    }

    // Adding an item keeps the existing nodes and appends a fresh one.
    #[test]
    fn reconcile_appends_new_item() {
        let mut ctx = WidgetCtx::new();
        let items = signal(vec![1, 2]);
        let src = items.clone();
        let list =
            ReactiveList::new(&mut ctx, move || src.get(), |n: &i32| *n, |c, _| leaf(c)).unwrap();
        let v1: Vec<NodeId> = list
            .state
            .borrow()
            .children
            .iter()
            .map(|c| c.node())
            .collect();

        items.set(vec![1, 2, 3]);

        let st = list.state.borrow();
        assert_eq!(st.children.len(), 3);
        let v2: Vec<NodeId> = st.children.iter().map(|c| c.node()).collect();
        assert_eq!(&v2[..2], &v1[..], "existing items keep their nodes");
    }

    // `with_gap` lays out the parent container's flex-column gap, so item N+1 sits `item_height + gap`
    // below item N instead of flush.
    #[test]
    fn with_gap_spaces_items_in_layout() {
        use crate::context::compute_layout;
        use layout_core::AvailableSpace;

        let mut ctx = WidgetCtx::new();
        let items = signal(vec![1i32, 2]);
        let src = items.clone();
        let list = ReactiveList::with_gap(
            &mut ctx,
            move || src.get(),
            |n: &i32| *n,
            |c, _| leaf(c),
            8.0,
        )
        .unwrap();
        let list_node = list.layout_node();
        compute_layout(
            &mut ctx,
            list_node,
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(200.0),
        )
        .unwrap();

        let st = list.state.borrow();
        let y0 = track_layout(&ctx, st.children[0].node()).unwrap().get().y;
        let y1 = track_layout(&ctx, st.children[1].node()).unwrap().get().y;
        assert_eq!(
            y1 - y0,
            18.0,
            "each leaf is 10px tall; an 8px gap pushes the second item to 18px, not flush at 10px"
        );
    }

    // A keyless reactive list (`for item in $items`, no `key` clause) reconciles by position: the item
    // previously at index 0/1 keeps its node when a third item is appended past the end.
    #[test]
    fn positional_reuses_nodes_on_append() {
        let mut ctx = WidgetCtx::new();
        let items = signal(vec![1, 2]);
        let src = items.clone();
        let list = ReactiveList::positional(&mut ctx, move || src.get(), |c, _| leaf(c)).unwrap();
        let v1: Vec<NodeId> = list
            .state
            .borrow()
            .children
            .iter()
            .map(|c| c.node())
            .collect();

        items.set(vec![1, 2, 3]);

        let st = list.state.borrow();
        assert_eq!(st.children.len(), 3);
        let v2: Vec<NodeId> = st.children.iter().map(|c| c.node()).collect();
        assert_eq!(
            &v2[..2],
            &v1[..],
            "the first two positions keep their nodes"
        );
    }
}
