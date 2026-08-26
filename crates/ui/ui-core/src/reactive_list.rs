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

use crate::context::{new_container, set_children, track_layout};
use crate::layout_item::{Child, LayoutItem, TrackedChildren, build_child, dispose_child};
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
    /// Runs the reconciled items along the horizontal axis instead of stacking them.
    ///
    /// Every constructor builds a column, because a list's own node exists before it is attached and
    /// cannot ask a parent it does not have yet. A `for` written inside a `row` reconciles into that row
    /// as a transparent fragment and never reaches here; one that *cannot* — inside a reactive `if`, say,
    /// which owns a node of its own — is boxed, and this is how it learns which way its items run.
    pub fn as_row(self) -> Self {
        crate::context::set_container_row(self.node);
        self
    }

    /// `source` reads the reactive item collection; `key` extracts a stable identity per item; `build`
    /// constructs one widget per item, creating its nodes against the live (thread-local) layout tree
    /// from inside the reconcile effect.
    pub fn new<Item, Key, S, K, B>(
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
        B: Fn(Item) -> Result<Box<dyn LayoutItem>, LayoutError> + 'static,
    {
        Self::build(
            LayoutStyle::new().flex_column().gap(gap),
            source,
            build,
            move |item: &Item, _idx: usize| hash_key(&key(item)),
        )
    }

    /// Keyed like [`new`](Self::new)/[`with_gap`](Self::with_gap), but the caller supplies the container's
    /// [`LayoutStyle`] — flex direction, gap, alignment. Use it for a horizontal reactive row (e.g. a bar's
    /// workspace chips), which the column-oriented constructors can't express.
    pub fn with_style<Item, Key, S, K, B>(
        container_style: LayoutStyle,
        source: S,
        key: K,
        build: B,
    ) -> Result<Self, LayoutError>
    where
        Key: Hash + 'static,
        Item: 'static,
        S: Fn() -> Vec<Item> + 'static,
        K: Fn(&Item) -> Key + 'static,
        B: Fn(Item) -> Result<Box<dyn LayoutItem>, LayoutError> + 'static,
    {
        Self::build(
            container_style,
            source,
            build,
            move |item: &Item, _idx: usize| hash_key(&key(item)),
        )
    }

    /// A keyless reactive list: `for item in $items` with no `key` clause. Reconciles by POSITION — the
    /// item at index `i` always reuses the node previously at index `i`, so an append/truncate reuses every
    /// surviving node cheaply, but a reorder rebuilds rather than moving nodes (no per-item identity without
    /// a key).
    pub fn positional<Item, S, B>(source: S, build: B, gap: f32) -> Result<Self, LayoutError>
    where
        Item: 'static,
        S: Fn() -> Vec<Item> + 'static,
        B: Fn(Item) -> Result<Box<dyn LayoutItem>, LayoutError> + 'static,
    {
        Self::build(
            LayoutStyle::new().flex_column().gap(gap),
            source,
            build,
            |_item: &Item, idx: usize| idx as u64,
        )
    }

    /// A keyed list whose builder receives a **live handle** to its item rather than a copy of it.
    ///
    /// The difference decides what a row is allowed to be. With an owned snapshot, a key that persists reuses
    /// the widget and the new value is discarded — so a row that must follow its data has to be keyed on that
    /// data, which rebuilds it and throws away whatever local state it held: a caret, a drag in progress, a
    /// scroll position. Keying on identity and reading the value through a handle separates the two questions —
    /// the key decides whether this is still the same row, the handle carries what that row now says.
    ///
    /// Use [`Self::new`] where a row genuinely *is* its value, which most lists are, and this where a row
    /// outlives its contents.
    pub fn keyed<Item, Key, S, K, B>(source: S, key: K, build: B) -> Result<Self, LayoutError>
    where
        Key: Hash + 'static,
        Item: Clone + 'static,
        S: Fn() -> Vec<Item> + 'static,
        K: Fn(&Item) -> Key + 'static,
        B: Fn(reactive_core::ReadSignal<Item>) -> Result<Box<dyn LayoutItem>, LayoutError>
            + 'static,
    {
        // One signal per live key, held outside `ListState` so that stays non-generic. `sync` runs for every
        // item on every reconcile and *before* the reuse decision, so a persisting row's handle already holds
        // the new value by the time anything reads it, and a new row's handle exists for `build` to read.
        let values: Rc<RefCell<HashMap<u64, RwSignal<Item>>>> =
            Rc::new(RefCell::new(HashMap::new()));

        let sync_values = Rc::clone(&values);
        let sync = move |item: &Item, k: u64| {
            let mut held = sync_values.borrow_mut();
            match held.get(&k) {
                Some(existing) => existing.set(item.clone()),
                None => {
                    held.insert(k, signal(item.clone()));
                }
            }
        };

        let key = Rc::new(key);
        let key_for_build = Rc::clone(&key);
        let build_values = Rc::clone(&values);
        Self::build_with_sync(
            LayoutStyle::new().flex_column(),
            source,
            move |item: Item| {
                let held = build_values
                    .borrow()
                    .get(&hash_key(&key_for_build(&item)))
                    .cloned()
                    .expect("sync inserts a handle for every item before build runs");
                build(held.read_only())
            },
            move |item: &Item, _idx: usize| hash_key(&key(item)),
            sync,
        )
    }

    /// Shared constructor: `keyer` erases both reconciliation modes (hashed key, or plain index) to a
    /// `u64` so `reconcile` doesn't need to know which mode produced it.
    fn build<Item, S, B, KeyFn>(
        container_style: LayoutStyle,
        source: S,
        build: B,
        keyer: KeyFn,
    ) -> Result<Self, LayoutError>
    where
        Item: 'static,
        S: Fn() -> Vec<Item> + 'static,
        B: Fn(Item) -> Result<Box<dyn LayoutItem>, LayoutError> + 'static,
        KeyFn: Fn(&Item, usize) -> u64 + 'static,
    {
        Self::build_with_sync(container_style, source, build, keyer, |_, _| {})
    }

    fn build_with_sync<Item, S, B, KeyFn, Sync>(
        container_style: LayoutStyle,
        source: S,
        build: B,
        keyer: KeyFn,
        sync: Sync,
    ) -> Result<Self, LayoutError>
    where
        Item: 'static,
        S: Fn() -> Vec<Item> + 'static,
        B: Fn(Item) -> Result<Box<dyn LayoutItem>, LayoutError> + 'static,
        KeyFn: Fn(&Item, usize) -> u64 + 'static,
        Sync: Fn(&Item, u64) + 'static,
    {
        let node = new_container(container_style, &[])?;
        let rect = track_layout(node).expect("list container is registered");
        let state = Rc::new(RefCell::new(ListState {
            node,
            children: Vec::new(),
            keys: Vec::new(),
        }));
        let version = signal(0u64);

        let eff_state = Rc::clone(&state);
        let eff_version = version;
        // Runs once now (builds the initial list) and again on every change to a signal `source` reads.
        let _effect = effect(move || {
            let items = source();
            reconcile(&eff_state, items, &keyer, &build, &sync);
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

fn reconcile<Item, KeyFn, B, Sync>(
    state: &Rc<RefCell<ListState>>,
    items: Vec<Item>,
    keyer: &KeyFn,
    build: &B,
    sync: &Sync,
) where
    KeyFn: Fn(&Item, usize) -> u64,
    B: Fn(Item) -> Result<Box<dyn LayoutItem>, LayoutError>,
    Sync: Fn(&Item, u64),
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

    let mut children: TrackedChildren = Vec::with_capacity(items.len());
    let mut keys: Vec<u64> = Vec::with_capacity(items.len());
    let mut nodes: Vec<NodeId> = Vec::with_capacity(items.len());

    for (idx, item) in items.into_iter().enumerate() {
        let k = keyer(&item, idx);
        // Before the reuse decision, so a row that keeps its key still sees the value it now carries. The
        // snapshot constructors pass a no-op here and keep the old behaviour exactly.
        sync(&item, k);
        let child = match old.remove(&k) {
            Some(existing) => existing,
            None => build_child(|| build(item).expect("reactive list item build")),
        };
        nodes.push(child.node());
        children.push(child);
        keys.push(k);
    }

    st.children = children;
    st.keys = keys;
    drop(st);

    // Reorder/insert/drop in the layout tree, then free the nodes of items that went away. set_children
    // first so the disposed nodes are detached before their owners withdraw and the ids are freed.
    let _ = set_children(container, &nodes);
    for (_, child) in old {
        dispose_child(&child);
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
        // The children are snapshotted and the borrow released *before* dispatch, because a handler is
        // allowed to change the list. A row that deletes itself, a strip that commits a drag-to-reorder — each
        // writes a signal this list's source reads, and the write flushes the reconcile effect synchronously,
        // which needs this same `RefCell`. Holding it across dispatch made every one of those a panic.
        //
        // Cloning is a handful of `Rc` bumps, and it is what makes the re-entrancy safe rather than merely
        // quiet: a child removed by the reconcile is still owned by this snapshot, so it finishes the event it
        // is in the middle of instead of being dropped underneath itself.
        let mut children = self.state.borrow().children.clone();
        dispatch_container_event(&mut children, event)
    }

    fn debug_name(&self) -> &'static str {
        "ReactiveList"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container::Container;
    use crate::context::reset_layout_runtime;
    use reactive_core::signal;

    fn leaf() -> Result<Box<dyn LayoutItem>, LayoutError> {
        Ok(Box::new(Container::new(
            LayoutStyle::new().width(10.0).height(10.0),
            vec![],
        )?))
    }

    // The effect runs once at construction, so the list is populated from the initial source.
    #[test]
    fn builds_initial_items() {
        reset_layout_runtime();
        let items = signal(vec![1, 2, 3]);
        let src = items;
        let list = ReactiveList::new(move || src.get(), |n: &i32| *n, |_| leaf(), 0.0).unwrap();
        assert_eq!(list.state.borrow().children.len(), 3);
    }

    // A reorder-plus-remove reuses the persisting items' nodes (keyed) and drops the gone one.
    #[test]
    fn reconcile_reuses_nodes_on_reorder_and_remove() {
        reset_layout_runtime();
        let items = signal(vec![1, 2, 3]);
        let src = items;
        let list = ReactiveList::new(move || src.get(), |n: &i32| *n, |_| leaf(), 0.0).unwrap();
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

        reset_layout_runtime();
        let items = signal(vec![1i32, 2]);
        let src = items;
        let list = ReactiveList::new(move || src.get(), |n: &i32| *n, |_| leaf(), 0.0).unwrap();
        let list_node = list.layout_node();
        compute_layout(
            list_node,
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(200.0),
        )
        .unwrap();
        assert!(
            track_layout(list.state.borrow().children[0].node())
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
            track_layout(n2).unwrap().get().height > 0.0,
            "the newly added item must be laid out after relayout_if_dirty"
        );
    }

    // Adding an item keeps the existing nodes and appends a fresh one.
    #[test]
    fn reconcile_appends_new_item() {
        reset_layout_runtime();
        let items = signal(vec![1, 2]);
        let src = items;
        let list = ReactiveList::new(move || src.get(), |n: &i32| *n, |_| leaf(), 0.0).unwrap();
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

        reset_layout_runtime();
        let items = signal(vec![1i32, 2]);
        let src = items;
        let list = ReactiveList::new(move || src.get(), |n: &i32| *n, |_| leaf(), 8.0).unwrap();
        let list_node = list.layout_node();
        compute_layout(
            list_node,
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(200.0),
        )
        .unwrap();

        let st = list.state.borrow();
        let y0 = track_layout(st.children[0].node()).unwrap().get().y;
        let y1 = track_layout(st.children[1].node()).unwrap().get().y;
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
        reset_layout_runtime();
        let items = signal(vec![1, 2]);
        let src = items;
        let list = ReactiveList::positional(move || src.get(), |_| leaf(), 0.0).unwrap();
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

    /// The trap this exists to remove: with an owned snapshot a persisting key reuses the widget and the new
    /// value is thrown away, so a row that must follow its data has to be keyed on that data — which rebuilds
    /// it, and takes its local state with it. `keyed` lets the row keep its widget *and* see the new value.
    #[test]
    fn a_keyed_row_keeps_its_widget_and_still_sees_its_new_value() {
        reset_layout_runtime();
        reactive_core::reset_runtime();

        #[derive(Clone)]
        struct Row {
            id: u32,
            text: &'static str,
        }

        let rows = signal(vec![Row {
            id: 1,
            text: "before",
        }]);
        let seen = Rc::new(RefCell::new(Vec::<&'static str>::new()));
        let builds = Rc::new(RefCell::new(0usize));

        let (sink, counter) = (Rc::clone(&seen), Rc::clone(&builds));
        let source = rows;
        let list = ReactiveList::keyed(
            move || source.get(),
            |row: &Row| row.id,
            move |held: reactive_core::ReadSignal<Row>| {
                *counter.borrow_mut() += 1;
                let sink = Rc::clone(&sink);
                // Reading the handle inside an effect is what a real row's text/style closure does.
                effect(move || sink.borrow_mut().push(held.get().text));
                Ok(Box::new(crate::Container::column(vec![])?) as Box<dyn LayoutItem>)
            },
        )
        .unwrap();

        let first = list.state.borrow().children[0].node();
        rows.set(vec![Row {
            id: 1,
            text: "after",
        }]);

        assert_eq!(*builds.borrow(), 1, "the row was built once, not rebuilt");
        assert_eq!(
            list.state.borrow().children[0].node(),
            first,
            "and kept the very node it had"
        );
        assert_eq!(
            *seen.borrow(),
            vec!["before", "after"],
            "while still seeing what it now says"
        );
    }

    /// A child handler is allowed to change the list it is in — a row that deletes itself, a strip that
    /// commits a drag-to-reorder. Both write a signal the source reads, which reconciles synchronously from
    /// inside this list's own `on_event`; holding the state borrow (or reading a node back through a widget
    /// that is mid-dispatch) made every one of those a panic rather than a feature.
    #[test]
    fn a_child_may_remove_itself_from_inside_its_own_handler() {
        use crate::context::compute_layout;
        use crate::styled_container::StyledContainer;
        use layout_core::AvailableSpace;
        use platform_core::{Event, PointerButton, PointerSource};

        reset_layout_runtime();
        let items = signal(vec![1i32, 2, 3]);
        let src = items;
        let pressed = items;
        let mut list = ReactiveList::new(
            move || src.get(),
            |n: &i32| *n,
            move |n: i32| {
                let items = pressed;
                Ok(Box::new(
                    StyledContainer::new(
                        LayoutStyle::new().width(50.0).height(20.0),
                        |_| Default::default(),
                        vec![],
                    )?
                    .on_press(move || {
                        items.set(items.peek().into_iter().filter(|x| *x != n).collect())
                    }),
                ) as Box<dyn LayoutItem>)
            },
            0.0,
        )
        .unwrap();
        compute_layout(
            list.layout_node(),
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(200.0),
        )
        .unwrap();

        // The second row: 20px tall each, so y=30 is inside it.
        let at = |y: f64| (25.0, y);
        let (x, y) = at(30.0);
        list.on_event(&Event::PointerPressed {
            x,
            y,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        });
        list.on_event(&Event::PointerReleased {
            x,
            y,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        });

        assert_eq!(items.peek(), vec![1, 3]);
        assert_eq!(list.state.borrow().children.len(), 2);
    }

    /// A row that leaves takes what its build made, including what a surviving handle would otherwise pin.
    ///
    /// The escaping clone is not contrived: `hot_state` does exactly this on purpose, holding a signal past
    /// the component that made it so the value survives a reload. Under the handle refcount that is
    /// indistinguishable from a leak — the storage lives as long as the clone, and rebuilding a row ten
    /// times leaves ten of them. The owner is what tells the two apart.
    #[test]
    fn rebuilding_a_row_does_not_accumulate_what_its_build_created() {
        reset_layout_runtime();
        let escaped: Rc<RefCell<Vec<RwSignal<usize>>>> = Rc::new(RefCell::new(Vec::new()));
        let items = signal(vec![0usize]);
        let source = items;
        let kept = Rc::clone(&escaped);
        let list = ReactiveList::new(
            move || source.get(),
            |n: &usize| *n,
            move |n: usize| {
                let per_row = signal(n);
                kept.borrow_mut().push(per_row);
                leaf()
            },
            0.0,
        )
        .unwrap();

        let one_row = reactive_core::live_signal_count();
        for generation in 1..=10usize {
            items.set(vec![generation]);
        }

        assert_eq!(escaped.borrow().len(), 11, "every generation built a row");
        assert_eq!(
            reactive_core::live_signal_count(),
            one_row,
            "eleven rows built, one row's worth of state alive"
        );
        assert_eq!(list.state.borrow().children.len(), 1);
    }
}
