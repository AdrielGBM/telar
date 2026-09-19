//! [`ReactiveList`]: children reconciled from a signal by key, reusing the widgets whose keys persist.

use std::cell::{Cell, RefCell};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use geometry_core::Rect;
use layout_core::{LayoutError, LayoutStyle, NodeId};
use motion_core::Curve;
use platform_core::Event;
use reactive_core::{Effect, RwSignal, effect, on_cleanup, signal};
use ui_tree::{Component, EventResult, RenderNode};

use crate::context::{new_container, set_children, track_layout};
use crate::layout_item::{BuildPass, Child, LayoutItem, TrackedChildren, dispose_child};
use crate::pointer::dispatch_container_event;
use crate::presence::{Transition, watch_exits};
use crate::row_keys::{Occurrences, index_by_key};
use crate::serial::Serial;

/// A key erased to a `u64` so the list state stays non-generic. A collision would reuse the wrong item's node, but a 64-bit hash makes that astronomically unlikely for the small, distinct keys a list uses.
fn hash_key<K: Hash>(k: &K) -> u64 {
    let mut h = DefaultHasher::new();
    k.hash(&mut h);
    h.finish()
}

/// Each key's value handle, with the token of the row that created it.
type KeyedValues<Item> = Rc<RefCell<HashMap<u64, (u64, RwSignal<Item>)>>>;

/// Shared reconciliation state, never borrowed across user code: a reconcile releases it to build rows and commits the new ones in one borrow at the end, so a build that panics leaves the last committed list standing.
struct ListState {
    node: NodeId,
    children: TrackedChildren,
    keys: Vec<u64>,
    exits: Option<ListExits>,
    /// The curve rows slide under when the layout moves them, once the list animates its layout.
    layout: Option<Curve>,
    reconciling: bool,
    /// Rows whose exits finished while a reconcile was building, dropped once it has committed.
    departed: Vec<u64>,
}

#[derive(Clone, Copy)]
struct ListExits {
    transition: Transition,
    /// Bumped when a row starts leaving, so the exit watcher subscribes to its progress.
    started: RwSignal<u64>,
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
    _exits: Option<Effect>,
}

impl ReactiveList {
    /// Runs the reconciled items along the horizontal axis instead of stacking them.
    ///
    /// Every constructor builds a column, because a list's own node exists before it is attached and cannot ask a parent it does not have yet. A `for` written inside a `row` reconciles into that row as a transparent fragment and never reaches here; one that *cannot* — inside a reactive `if`, say, which owns a node of its own — is boxed, and this is how it learns which way its items run.
    pub fn as_row(self) -> Self {
        crate::context::set_container_row(self.node);
        self
    }

    /// Plays `transition` as rows arrive and leave: a removed row stays mounted, taking no input, until its exit finishes, and a key that returns mid-exit turns back as the same row.
    pub fn with_transition(mut self, transition: Transition) -> Self {
        let started = signal(0u64);
        self.state.borrow_mut().exits = Some(ListExits {
            transition,
            started,
        });
        let watched = Rc::clone(&self.state);
        let removed = Rc::clone(&self.state);
        let version = self.version;
        self._exits = Some(watch_exits(
            move || {
                started.get();
                let st = watched.borrow();
                st.keys
                    .iter()
                    .zip(&st.children)
                    .filter_map(|(k, child)| {
                        child
                            .mount()
                            .filter(|mount| mount.is_leaving_now())
                            .map(|mount| (*k, mount))
                    })
                    .collect()
            },
            move |done| {
                if drop_departed(&removed, &done) {
                    version.update(|v| *v = v.wrapping_add(1));
                }
            },
        ));
        self
    }

    /// Slides rows to wherever the layout moves them, under `curve`, instead of letting them jump; with [`with_transition`](Self::with_transition), a leaving row also gives up its space as its exit starts, so the list closes over the exit's duration rather than jumping once it ends.
    pub fn animate_layout(self, curve: impl Into<Curve>) -> Self {
        let curve = curve.into();
        let mut st = self.state.borrow_mut();
        st.layout = Some(curve);
        for child in &mut st.children {
            child.animate_layout(curve);
        }
        drop(st);
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
        // One signal per row, held outside `ListState` so that stays non-generic. Each is created in its row's build, so it belongs to the row's owner and is freed with the row, and `sync` runs before the reuse decision so a persisting row's handle already holds the new value. The token tells a row's cleanup whether the entry is still its own.
        let values: KeyedValues<Item> = Rc::new(RefCell::new(HashMap::new()));
        let next_token = Rc::new(Cell::new(0u64));

        let sync_values = Rc::clone(&values);
        let sync = move |item: &Item, k: u64| {
            let existing = sync_values.borrow().get(&k).map(|(_, value)| *value);
            if let Some(existing) = existing {
                existing.set(item.clone());
            }
        };

        let build_values = Rc::clone(&values);
        Self::build_with_sync(
            LayoutStyle::new().flex_column(),
            source,
            move |item: Item, k: u64| {
                let token = next_token.replace(next_token.get().wrapping_add(1));
                let held = signal(item);
                build_values.borrow_mut().insert(k, (token, held));
                let values = Rc::downgrade(&build_values);
                on_cleanup(move || {
                    let Some(values) = values.upgrade() else {
                        return;
                    };
                    let mut values = values.borrow_mut();
                    if values.get(&k).is_some_and(|(owner, _)| *owner == token) {
                        values.remove(&k);
                    }
                });
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
        Self::build_with_sync(
            container_style,
            source,
            move |item, _| build(item),
            keyer,
            |_, _| {},
        )
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
        B: Fn(Item, u64) -> Result<Box<dyn LayoutItem>, LayoutError> + 'static,
        KeyFn: Fn(&Item, usize) -> u64 + 'static,
        Sync: Fn(&Item, u64) + 'static,
    {
        let node = new_container(container_style, &[])?;
        let rect = track_layout(node).expect("list container is registered");
        let state = Rc::new(RefCell::new(ListState {
            node,
            children: Vec::new(),
            keys: Vec::new(),
            exits: None,
            layout: None,
            reconciling: false,
            departed: Vec::new(),
        }));
        let version = signal(0u64);

        let eff_state = Rc::clone(&state);
        let eff_version = version;
        let serial = Serial::new();
        let _effect = effect(move || {
            let items = source();
            serial.run(items, |items| {
                reconcile(&eff_state, items, &keyer, &build, &sync)
            });
            eff_version.update(|v| *v = v.wrapping_add(1));
        });

        Ok(Self {
            node,
            rect,
            state,
            version,
            _effect,
            _exits: None,
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
    B: Fn(Item, u64) -> Result<Box<dyn LayoutItem>, LayoutError>,
    Sync: Fn(&Item, u64),
{
    let (container, exits, layout, old_keys, old_children) = {
        let mut st = state.borrow_mut();
        st.reconciling = true;
        (
            st.node,
            st.exits,
            st.layout,
            st.keys.clone(),
            st.children.clone(),
        )
    };
    let building = Building(state);

    // Indexed by key hash so a persisting key reuses its widget and node.
    let (mut old, strays) = index_by_key(old_keys.iter().copied(), old_children);
    let mut occurrences = Occurrences::new();

    let mut children: TrackedChildren = Vec::with_capacity(items.len());
    let mut keys: Vec<u64> = Vec::with_capacity(items.len());

    // A row build that panics leaves the committed list as it was, so the pass frees the rows it had already built along with what the failed row left.
    let mut pass = BuildPass::start();
    // Turned back only once every row has built, so a pass that fails leaves a leaving row leaving.
    let mut returning = Vec::new();
    for (idx, item) in items.into_iter().enumerate() {
        let k = occurrences.identify(keyer(&item, idx));
        // Before the reuse decision, so a row keeping its key sees the value it now carries.
        sync(&item, k);
        let child = match old.remove(&k) {
            Some(existing) => {
                returning.extend(existing.mount());
                existing
            }
            None => {
                let mut built = pass
                    .build_child(|| build(item, k))
                    .expect("reactive list item build");
                if let Some(exits) = exits {
                    built.present(exits.transition, true);
                }
                if let Some(curve) = layout {
                    built.animate_layout(curve);
                }
                built
            }
        };
        children.push(child);
        keys.push(k);
    }
    pass.keep();
    for mount in returning {
        mount.come_back();
    }

    if let Some(exits) = exits {
        let started = keep_departures(&old_keys, &mut old, &mut children, &mut keys, exits);
        if started {
            exits.started.update(|n| *n = n.wrapping_add(1));
        }
    }

    let nodes = flow(&children, layout.is_some());
    let departed = {
        let mut st = state.borrow_mut();
        st.children = children;
        st.keys = keys;
        std::mem::take(&mut st.departed)
    };
    drop(building);

    // `set_children` first, so disposed nodes are detached before their owners withdraw and the ids are freed.
    let _ = set_children(container, &nodes);
    for child in old.into_values().chain(strays) {
        dispose_child(&child);
    }
    if !departed.is_empty() {
        drop_departed(state, &departed);
    }
}

/// The nodes that take part in the list's layout. A list that animates its layout leaves a leaving row out, so the rows after it close the gap while it plays its exit where it was.
fn flow(children: &[Child], closes_gaps: bool) -> Vec<NodeId> {
    children
        .iter()
        .filter(|child| {
            let out = closes_gaps && child.is_leaving();
            child.set_out_of_flow(out);
            !out
        })
        .map(Child::node)
        .collect()
}

/// Marks a reconcile as building rows until it drops, including by an unwind.
struct Building<'a>(&'a Rc<RefCell<ListState>>);

impl Drop for Building<'_> {
    fn drop(&mut self) {
        self.0.borrow_mut().reconciling = false;
    }
}

/// Moves each removed row that has an exit to play out of `old` and back into the list, next to the row it followed, so it leaves from the place it held. Answers whether any exit started.
fn keep_departures(
    old_keys: &[u64],
    old: &mut HashMap<u64, Child>,
    children: &mut TrackedChildren,
    keys: &mut Vec<u64>,
    exits: ListExits,
) -> bool {
    let present: HashSet<u64> = keys.iter().copied().collect();
    let mut after: HashMap<Option<u64>, Vec<(u64, Child)>> = HashMap::new();
    let mut previous = None;
    let mut started = false;
    for &k in old_keys {
        if present.contains(&k) {
            previous = Some(k);
            continue;
        }
        let Some(child) = old.get_mut(&k) else {
            continue;
        };
        let already = child.is_leaving();
        if !already && !child.present(exits.transition, false).leave() {
            continue;
        }
        started |= !already;
        let child = old.remove(&k).expect("the row was just found");
        after.entry(previous).or_default().push((k, child));
    }
    if after.is_empty() {
        return started;
    }
    let placed: Vec<(u64, Child)> = keys.drain(..).zip(children.drain(..)).collect();
    let mut place = |slot: Option<u64>, keys: &mut Vec<u64>, children: &mut TrackedChildren| {
        for (k, child) in after.remove(&slot).unwrap_or_default() {
            keys.push(k);
            children.push(child);
        }
    };
    place(None, keys, children);
    for (k, child) in placed {
        keys.push(k);
        children.push(child);
        place(Some(k), keys, children);
    }
    started
}

/// Disposes the leaving rows among `done` whose exits have finished, answering whether any went. Mid-reconcile it only notes them, since that reconcile is about to commit a list built from the rows as they were.
fn drop_departed(state: &Rc<RefCell<ListState>>, done: &[u64]) -> bool {
    let (container, nodes, gone) = {
        let mut st = state.borrow_mut();
        if st.reconciling {
            st.departed.extend_from_slice(done);
            return false;
        }
        let keys = std::mem::take(&mut st.keys);
        let children = std::mem::take(&mut st.children);
        let mut gone = Vec::new();
        for (k, child) in keys.into_iter().zip(children) {
            if child.is_leaving() && done.contains(&k) {
                gone.push(child);
            } else {
                st.keys.push(k);
                st.children.push(child);
            }
        }
        let nodes = flow(&st.children, st.layout.is_some());
        (st.node, nodes, gone)
    };
    if gone.is_empty() {
        return false;
    }
    let _ = set_children(container, &nodes);
    for child in &gone {
        dispose_child(child);
    }
    true
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
        let content = RenderNode::group(st.children.iter().map(Child::boundary));
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
