//! [`ReactiveList`]: children reconciled from a signal by key, reusing the widgets whose keys persist.

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

/// A key erased to a `u64` so the list state stays non-generic. A collision would reuse the wrong item's node, but a 64-bit hash makes that astronomically unlikely for the small, distinct keys a list uses.
fn hash_key<K: Hash>(k: &K) -> u64 {
    let mut h = DefaultHasher::new();
    k.hash(&mut h);
    h.finish()
}

/// Shared reconciliation state: the container node plus the current items in order and their key hashes. Mutated by the reconcile effect (during the reactive flush) and read by `view`/`on_event` (during render/dispatch) — never concurrently, so the `RefCell` never double-borrows.
struct ListState {
    node: NodeId,
    children: TrackedChildren,
    keys: Vec<u64>,
}

/// A reactive list: `for item in $items key id` (or, keyless, `for item in $items`) in `.rsx`. Re-runs its source reactively and reconciles the item widgets — reused keys/positions keep their node/widget, new ones are built, gone ones are disposed, and the layout children are reordered — instead of rebuilding the whole block on every change. `new`/`with_gap` reconcile by key (identity-stable); `positional`/ `positional_with_gap` reconcile by index (no `key` clause needed, cheap append/truncate).
pub struct ReactiveList {
    node: NodeId,
    rect: RwSignal<Rect>,
    state: Rc<RefCell<ListState>>,
    // Bumped on every reconcile, so `view()` re-emits the new child group.
    version: RwSignal<u64>,
    // Keeps the reconcile effect alive for the widget's lifetime.
    _effect: Effect,
}

impl ReactiveList {
    /// Runs the reconciled items along the horizontal axis instead of stacking them.
    ///
    /// Every constructor builds a column, because a list's own node exists before it is attached and cannot ask a parent it does not have yet. A `for` written inside a `row` reconciles into that row as a transparent fragment and never reaches here; one that *cannot* — inside a reactive `if`, say, which owns a node of its own — is boxed, and this is how it learns which way its items run.
    pub fn as_row(self) -> Self {
        crate::context::set_container_row(self.node);
        self
    }

    /// `source` reads the reactive item collection; `key` extracts a stable identity per item; `build` constructs one widget per item, creating its nodes against the live (thread-local) layout tree from inside the reconcile effect.
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

    /// Keyed like [`new`](Self::new)/`with_gap`, but the caller supplies the container's [`LayoutStyle`] — flex direction, gap, alignment. Use it for a horizontal reactive row (e.g. a bar's workspace chips), which the column-oriented constructors can't express.
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

    /// A keyless reactive list: `for item in $items` with no `key` clause. Reconciles by POSITION — the item at index `i` always reuses the node previously at index `i`, so an append/truncate reuses every surviving node cheaply, but a reorder rebuilds rather than moving nodes (no per-item identity without a key).
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
    /// The difference decides what a row is allowed to be. With an owned snapshot, a key that persists reuses the widget and the new value is discarded — so a row that must follow its data has to be keyed on that data, which rebuilds it and throws away whatever local state it held: a caret, a drag in progress, a scroll position. Keying on identity and reading the value through a handle separates the two questions — the key decides whether this is still the same row, the handle carries what that row now says.
    ///
    /// Use [`Self::new`] where a row genuinely *is* its value, which most lists are, and this where a row outlives its contents.
    pub fn keyed<Item, Key, S, K, B>(source: S, key: K, build: B) -> Result<Self, LayoutError>
    where
        Key: Hash + 'static,
        Item: Clone + 'static,
        S: Fn() -> Vec<Item> + 'static,
        K: Fn(&Item) -> Key + 'static,
        B: Fn(reactive_core::ReadSignal<Item>) -> Result<Box<dyn LayoutItem>, LayoutError>
            + 'static,
    {
        // One signal per live key, held outside `ListState` so that stays non-generic. `sync` runs before the reuse decision, so a persisting row's handle already holds the new value and a new row's exists for `build`.
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

    /// Shared constructor: `keyer` erases both reconciliation modes (hashed key, or plain index) to a `u64` so `reconcile` doesn't need to know which mode produced it.
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
        // Runs once now to build the initial list, and again on every change to a signal `source` reads.
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

    // Indexed by key hash so a persisting key reuses its widget and node.
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
        // Before the reuse decision, so a row keeping its key sees the value it now carries.
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

    // `set_children` first, so disposed nodes are detached before their owners withdraw and the ids are freed.
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
        // Subscribes to reconciles, so the child group re-emits on add, remove or reorder.
        self.version.get();
        let _ = self.rect.get();
        let st = self.state.borrow();
        let content = RenderNode::group(st.children.iter().map(|c| c.segment.boundary()));
        crate::element::wrap(self.node, content)
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        // Snapshotted and the borrow released before dispatch, because a handler may change the list: a row that deletes itself writes a signal this list's source reads, and the write flushes the reconcile effect synchronously through this same `RefCell`. The clone is a handful of `Rc` bumps, and it lets a child removed by the reconcile finish the event it is in the middle of.
        let mut children = self.state.borrow().children.clone();
        dispatch_container_event(&mut children, event)
    }

    fn debug_name(&self) -> &'static str {
        "ReactiveList"
    }
}

#[cfg(test)]
#[path = "reactive_list_test.rs"]
mod tests;
