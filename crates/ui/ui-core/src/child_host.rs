//! Transparent reactive regions inside a container.
//!
//! A container's children are a sequence of [`ChildSlot`]s, each either a fixed widget or a reactive
//! *fragment* — a keyed, reconciling region that has **no box of its own**. A fragment reconciles its
//! items directly into the *host container's* layout node, so the items are real siblings of the static
//! children: they inherit the host's flex direction, gap and alignment, resolved at layout time (like the
//! web's transparent `array.map(...)`, which Taffy can't express via `display:contents`). This is what
//! makes a reactive `for`/`if` flow horizontally inside a `row` without a wrapper imposing a column axis.
//!
//! Reconciliation reuses the host node via [`set_children`] (which replaces all of a node's children in
//! order), re-flattening every slot on each change — so several fragments and static siblings interleave
//! correctly. Compare [`crate::reactive_list::ReactiveList`], which is the boxed, standalone variant.

use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use layout_core::{LayoutError, NodeId};
use platform_core::Event;
use reactive_core::{Effect, RwSignal, effect, signal};
use ui_tree::{EventResult, RenderNode};

use crate::context::{container_is_row, remove_node, set_children, set_leading_margin};
use crate::layout_item::{Child, LayoutItem, make_child};
use crate::pointer::dispatch_container_event;

fn hash_key<K: Hash>(k: &K) -> u64 {
    let mut h = DefaultHasher::new();
    k.hash(&mut h);
    h.finish()
}

/// One child position in a container: a fixed widget, or a reactive fragment (built lazily once the host
/// node exists). Produced by [`ChildSlot::stat`] / [`fragment`] / [`fragment_positional`].
pub enum ChildSlot {
    Static(Box<dyn LayoutItem>),
    Dynamic(FragmentSpec),
}

impl ChildSlot {
    /// A fixed widget slot — the transpiler's `Slot::stat(box_item(x))` for a static child of a container
    /// that also holds a reactive region.
    pub fn stat(item: Box<dyn LayoutItem>) -> Self {
        ChildSlot::Static(item)
    }
}

/// A generic-erased reactive region. Captures `source`/`key`/`build` behind one closure that, given the
/// shared host state and this slot's index, wires the reconcile [`Effect`]. Erasing here keeps
/// [`HostState`] non-generic (a container can hold fragments over different item types). `gap` is the
/// per-item spacing (`0.0` = none), applied as a main-axis leading margin so the region stays transparent.
pub struct FragmentSpec {
    install: Box<dyn FnOnce(Rc<RefCell<HostState>>, usize) -> Effect>,
    gap: f32,
}

/// A keyed reactive region — `for item in $items key <expr>` (identity-stable reconciliation).
pub fn fragment<Item, Key, S, K, B>(source: S, key: K, build: B) -> ChildSlot
where
    Key: Hash + 'static,
    Item: 'static,
    S: Fn() -> Vec<Item> + 'static,
    K: Fn(&Item) -> Key + 'static,
    B: Fn(Item) -> Result<Box<dyn LayoutItem>, LayoutError> + 'static,
{
    make_fragment(
        source,
        move |item: &Item, _idx: usize| hash_key(&key(item)),
        build,
        0.0,
    )
}

/// A keyed reactive region with per-item spacing — `for item in $items key <expr> gap:N`. The gap is laid
/// out as a main-axis leading margin between consecutive items (see [`reconcile_slot`]), so the region
/// still flows transparently in the host's direction (horizontal in a `row`) instead of a boxed list.
pub fn fragment_gap<Item, Key, S, K, B>(source: S, key: K, build: B, gap: f32) -> ChildSlot
where
    Key: Hash + 'static,
    Item: 'static,
    S: Fn() -> Vec<Item> + 'static,
    K: Fn(&Item) -> Key + 'static,
    B: Fn(Item) -> Result<Box<dyn LayoutItem>, LayoutError> + 'static,
{
    make_fragment(
        source,
        move |item: &Item, _idx: usize| hash_key(&key(item)),
        build,
        gap,
    )
}

/// A keyless reactive region — `for item in $items` (reconciles by position).
pub fn fragment_positional<Item, S, B>(source: S, build: B) -> ChildSlot
where
    Item: 'static,
    S: Fn() -> Vec<Item> + 'static,
    B: Fn(Item) -> Result<Box<dyn LayoutItem>, LayoutError> + 'static,
{
    make_fragment(source, |_item: &Item, idx: usize| idx as u64, build, 0.0)
}

/// A keyless reactive region with per-item spacing — `for item in $items gap:N` (reconciles by position).
pub fn fragment_positional_gap<Item, S, B>(source: S, build: B, gap: f32) -> ChildSlot
where
    Item: 'static,
    S: Fn() -> Vec<Item> + 'static,
    B: Fn(Item) -> Result<Box<dyn LayoutItem>, LayoutError> + 'static,
{
    make_fragment(source, |_item: &Item, idx: usize| idx as u64, build, gap)
}

fn make_fragment<Item, S, B, KeyFn>(source: S, keyer: KeyFn, build: B, gap: f32) -> ChildSlot
where
    Item: 'static,
    S: Fn() -> Vec<Item> + 'static,
    B: Fn(Item) -> Result<Box<dyn LayoutItem>, LayoutError> + 'static,
    KeyFn: Fn(&Item, usize) -> u64 + 'static,
{
    let install = Box::new(
        move |state: Rc<RefCell<HostState>>, index: usize| -> Effect {
            // Runs once now (builds the initial items) and again on every change to a signal `source` reads.
            effect(move || {
                let items = source();
                reconcile_slot(&state, index, items, &keyer, &build);
            })
        },
    );
    ChildSlot::Dynamic(FragmentSpec { install, gap })
}

/// The reconciled items of one dynamic slot, plus their key hashes (mirrors `ReactiveList`'s `ListState`).
/// `gap` (px) is spaced between consecutive items as a main-axis leading margin — `left` when `is_row`, else
/// `top`; both are captured once from the host when the slot is built (`0.0` gap = no margin work).
#[derive(Default)]
struct DynState {
    items: Vec<Child>,
    keys: Vec<u64>,
    gap: f32,
    is_row: bool,
}

enum SlotState {
    Static(Child),
    Dynamic(DynState),
}

/// Shared host state: the container's layout node plus its slots in order. Mutated by fragment reconcile
/// effects (during the reactive flush) and read by the container's `view`/`on_event` (during
/// render/dispatch) — never concurrently, so the `RefCell` never double-borrows.
struct HostState {
    node: NodeId,
    slots: Vec<SlotState>,
    version: RwSignal<u64>,
}

fn flatten_nodes(slots: &[SlotState]) -> Vec<NodeId> {
    let mut nodes = Vec::new();
    for slot in slots {
        match slot {
            SlotState::Static(child) => nodes.push(child.node()),
            SlotState::Dynamic(dyn_state) => nodes.extend(dyn_state.items.iter().map(Child::node)),
        }
    }
    nodes
}

fn collect_children(slots: &[SlotState]) -> Vec<Child> {
    let mut out = Vec::new();
    for slot in slots {
        match slot {
            SlotState::Static(child) => out.push(child.clone()),
            SlotState::Dynamic(dyn_state) => out.extend(dyn_state.items.iter().cloned()),
        }
    }
    out
}

fn reconcile_slot<Item, KeyFn, B>(
    state: &Rc<RefCell<HostState>>,
    index: usize,
    items: Vec<Item>,
    keyer: &KeyFn,
    build: &B,
) where
    KeyFn: Fn(&Item, usize) -> u64,
    B: Fn(Item) -> Result<Box<dyn LayoutItem>, LayoutError>,
{
    let mut st = state.borrow_mut();

    // Index this slot's current items by key hash so a persisting key reuses its widget/node.
    let (old_items, old_keys) = match &mut st.slots[index] {
        SlotState::Dynamic(dyn_state) => (
            std::mem::take(&mut dyn_state.items),
            std::mem::take(&mut dyn_state.keys),
        ),
        SlotState::Static(_) => unreachable!("a fragment slot is never static"),
    };
    let mut old: HashMap<u64, Child> = HashMap::new();
    for (k, child) in old_keys.into_iter().zip(old_items) {
        old.entry(k).or_insert(child);
    }

    let mut new_items: Vec<Child> = Vec::with_capacity(items.len());
    let mut keys: Vec<u64> = Vec::with_capacity(items.len());
    for (idx, item) in items.into_iter().enumerate() {
        let k = keyer(&item, idx);
        let child = match old.remove(&k) {
            Some(existing) => existing,
            None => make_child(build(item).expect("fragment item build")),
        };
        new_items.push(child);
        keys.push(k);
    }

    // Capture this slot's item nodes (in order) and its gap/axis before writing them back, so the gap can be
    // re-applied as a per-item margin below without re-borrowing.
    let (gap, is_row, item_nodes) = if let SlotState::Dynamic(dyn_state) = &mut st.slots[index] {
        let item_nodes: Vec<NodeId> = new_items.iter().map(Child::node).collect();
        dyn_state.items = new_items;
        dyn_state.keys = keys;
        (dyn_state.gap, dyn_state.is_row, item_nodes)
    } else {
        (0.0, false, Vec::new())
    };

    // Reorder/insert/drop across the whole host node, then free the nodes of items that went away.
    let nodes = flatten_nodes(&st.slots);
    let node = st.node;
    let version = st.version.clone();
    drop(st);

    let _ = set_children(node, &nodes);
    // A `for … gap:N` has no box to carry a container gap, so the spacing lives on the items: every item but
    // the first gets a leading main-axis margin of `gap`. Re-applied each reconcile (not baked at build) so a
    // reordered item that lands first loses its margin and one that leaves the front gains it.
    if gap != 0.0 {
        for (i, &item) in item_nodes.iter().enumerate() {
            set_leading_margin(item, is_row, if i == 0 { 0.0 } else { gap });
        }
    }
    for (_, child) in old {
        remove_node(child.node());
    }
    version.update(|v| *v = v.wrapping_add(1));
}

/// The dynamic child store a container embeds when it holds at least one reactive fragment. Owns the
/// slots and keeps the reconcile effects alive; the container delegates render/hit-test to it.
pub(crate) struct DynHost {
    state: Rc<RefCell<HostState>>,
    version: RwSignal<u64>,
    _effects: Vec<Effect>,
}

impl DynHost {
    /// `node` is the already-registered container node; `slots` are its children in order. Fragments
    /// reconcile their items straight into `node`.
    pub(crate) fn build(node: NodeId, slots: Vec<ChildSlot>) -> Result<Self, LayoutError> {
        let version = signal(0u64);
        let state = Rc::new(RefCell::new(HostState {
            node,
            slots: Vec::with_capacity(slots.len()),
            version: version.clone(),
        }));

        // The host's flex axis is fixed by now (its style, class-driven direction included, was set when the
        // node was created), so a gap fragment can capture which margin edge to space its items on, once.
        let host_is_row = container_is_row(node);

        // Materialize every slot in order first (statics as children, dynamics as empty placeholders) so
        // that when a fragment effect runs it flattens the complete slot structure, keeping sibling order.
        let mut specs: Vec<(usize, FragmentSpec)> = Vec::new();
        {
            let mut st = state.borrow_mut();
            for slot in slots {
                let index = st.slots.len();
                match slot {
                    ChildSlot::Static(item) => st.slots.push(SlotState::Static(make_child(item))),
                    ChildSlot::Dynamic(spec) => {
                        st.slots.push(SlotState::Dynamic(DynState {
                            gap: spec.gap,
                            is_row: host_is_row,
                            ..Default::default()
                        }));
                        specs.push((index, spec));
                    }
                }
            }
        }

        let effects: Vec<Effect> = specs
            .into_iter()
            .map(|(index, spec)| (spec.install)(state.clone(), index))
            .collect();

        let nodes = flatten_nodes(&state.borrow().slots);
        let _ = set_children(node, &nodes);

        Ok(Self {
            state,
            version,
            _effects: effects,
        })
    }

    /// The current children's render boundaries, in order. Subscribes to reconciles so the container's
    /// `view()` re-emits the new/reordered set.
    pub(crate) fn child_boundaries(&self) -> Vec<RenderNode> {
        self.version.get();
        let st = self.state.borrow();
        let mut out = Vec::new();
        for slot in &st.slots {
            match slot {
                SlotState::Static(child) => out.push(child.segment.boundary()),
                SlotState::Dynamic(dyn_state) => {
                    out.extend(dyn_state.items.iter().map(|c| c.segment.boundary()))
                }
            }
        }
        out
    }

    /// Dispatch an event to the current children (cheap `Rc`-clone flatten, so hit-testing sees every
    /// live item in order).
    pub(crate) fn dispatch(&self, event: &Event) -> EventResult {
        let mut children = collect_children(&self.state.borrow().slots);
        dispatch_container_event(&mut children, event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container::Container;
    use crate::context::{compute_layout, reset_layout_runtime, track_layout};
    use layout_core::{AvailableSpace, LayoutStyle};
    use reactive_core::signal;
    use ui_tree::Component;

    fn leaf10() -> Box<dyn LayoutItem> {
        Box::new(Container::new(LayoutStyle::new().width(10.0).height(10.0), vec![]).unwrap())
    }

    // Returns a 10×10 leaf together with its layout node, so a test can read its laid-out rect.
    fn leaf10_node() -> (NodeId, Box<dyn LayoutItem>) {
        let c = Container::new(LayoutStyle::new().width(10.0).height(10.0), vec![]).unwrap();
        (c.layout_node(), Box::new(c))
    }

    fn group_len(node: &RenderNode) -> usize {
        match node {
            RenderNode::Group { children, .. } => children.len(),
            _ => panic!("expected Group"),
        }
    }

    // A fragment's items flatten between the static siblings and reconcile on source change: the host's
    // `view()` group holds `static + dynamic + static`, growing and shrinking with the signal.
    #[test]
    fn fragment_children_flatten_and_reconcile() {
        reset_layout_runtime();
        let items = signal(vec![1u32, 2, 3]);
        let src = items.clone();
        let container = Container::from_slots(
            LayoutStyle::new().flex_row(),
            vec![
                ChildSlot::stat(leaf10()),
                fragment(move || src.get(), |n: &u32| *n, |_n| Ok(leaf10())),
                ChildSlot::stat(leaf10()),
            ],
        )
        .unwrap();

        assert_eq!(group_len(&container.view()), 5, "2 static + 3 dynamic");

        // Outside a batch, `set` flushes the reconcile effect immediately.
        items.set(vec![9]);
        assert_eq!(group_len(&container.view()), 3, "2 static + 1 dynamic");

        items.set(vec![9, 8, 7, 6]);
        assert_eq!(group_len(&container.view()), 6, "2 static + 4 dynamic");
    }

    // The load-bearing property of C: the fragment's items are laid out as real siblings of the static
    // children, IN THE HOST'S ROW DIRECTION and BETWEEN the two statics — not stacked in a private column.
    #[test]
    fn fragment_items_flow_in_host_direction_between_statics() {
        reset_layout_runtime();
        let (s0, static0) = leaf10_node();
        let (s1, static1) = leaf10_node();
        let built: Rc<RefCell<Vec<NodeId>>> = Rc::new(RefCell::new(Vec::new()));
        let sink = built.clone();
        let items = signal(vec![1u32, 2, 3]);
        let src = items.clone();

        let container = Container::from_slots(
            LayoutStyle::new().flex_row(),
            vec![
                ChildSlot::stat(static0),
                fragment(
                    move || src.get(),
                    |n: &u32| *n,
                    move |_n| {
                        let (node, item) = leaf10_node();
                        sink.borrow_mut().push(node);
                        Ok(item)
                    },
                ),
                ChildSlot::stat(static1),
            ],
        )
        .unwrap();

        compute_layout(
            container.layout_node(),
            AvailableSpace::Definite(500.0),
            AvailableSpace::Definite(50.0),
        )
        .unwrap();

        let frag = built.borrow().clone();
        assert_eq!(frag.len(), 3);
        let x = |node: NodeId| track_layout(node).unwrap().get().x;
        // static0 → frag[0] → frag[1] → frag[2] → static1, strictly left-to-right: horizontal + interleaved.
        let xs = [x(s0), x(frag[0]), x(frag[1]), x(frag[2]), x(s1)];
        for pair in xs.windows(2) {
            assert!(
                pair[1] > pair[0],
                "children must advance along the row (got x sequence {xs:?})"
            );
        }
        // All share the row's top: same y, so they are siblings on one line, not a nested column.
        let y = |node: NodeId| track_layout(node).unwrap().get().y;
        assert!((y(frag[0]) - y(s0)).abs() < 0.01 && (y(s1) - y(frag[2])).abs() < 0.01);
    }

    // A fragment chip added AFTER the initial layout (workspace chips built when the socket answers, well
    // after the bar first laid out) must still fire its handler. Mirrors the workspaces shape: a `from_slots`
    // row is the component root, hosting a `fragment_gap` whose single-`box` body is a bare `StyledContainer`.
    #[test]
    fn pressing_a_fragment_chip_added_after_layout_fires_its_handler() {
        use crate::context::{new_container, relayout_if_dirty};
        use crate::styled_container::StyledContainer;
        use layout_core::{AlignItems, JustifyContent};
        use platform_core::{Event, PointerButton, PointerSource};
        use renderer_core::RectStyle;

        reset_layout_runtime();
        let fired = Rc::new(std::cell::Cell::new(0i32));
        let ids = signal(Vec::<i32>::new()); // empty at first, like the snapshot before the socket answers
        let src = ids.clone();
        let sink = fired.clone();

        // The `row` is the module root itself (the fixed `generate_root` returns the branch element bare).
        let mut row = Container::from_slots(
            LayoutStyle::new()
                .flex_row()
                .align_items(AlignItems::CENTER),
            vec![fragment_gap(
                move || src.get(),
                |id: &i32| *id as u64,
                move |id| {
                    let sink = sink.clone();
                    let chip = StyledContainer::new(
                        LayoutStyle::new()
                            .flex_column()
                            .width(24.0)
                            .height(24.0)
                            .align_items(AlignItems::CENTER)
                            .justify_content(JustifyContent::CENTER),
                        move |_| RectStyle::default(),
                        vec![],
                    )?
                    .on_press(move || sink.set(id));
                    Ok(Box::new(chip) as Box<dyn LayoutItem>)
                },
                8.0,
            )],
        )
        .unwrap();

        let root = new_container(
            LayoutStyle::new().flex_row().width(200.0).height(24.0),
            &[row.layout_node()],
        )
        .unwrap();
        compute_layout(
            root,
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(24.0),
        )
        .unwrap();

        // Data arrives after layout: the fragment reconciles (builds 3 chips) and the runtime relayouts them.
        ids.set(vec![7, 8, 9]);
        relayout_if_dirty();

        // Click the first chip: no leading gap, so it sits at x∈[0,24), y∈[0,24). Press then release inside it.
        let press = |x: f64, y: f64| Event::PointerPressed {
            x,
            y,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        };
        let release = |x: f64, y: f64| Event::PointerReleased {
            x,
            y,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        };
        row.on_event(&press(12.0, 12.0));
        row.on_event(&release(12.0, 12.0));

        assert_eq!(
            fired.get(),
            7,
            "clicking the first workspace-style chip should fire its on_press"
        );
    }

    // Regression (workspaces "chips don't fill the bar height"): a `from_slots` stretch row of bare
    // `StyledContainer` chips, placed in a STRETCH zone, must stretch each chip to the full zone height —
    // no injected flex-column around the root or the chips to trap them at content height.
    #[test]
    fn stretch_row_of_fragment_chips_fills_the_zone_height() {
        use crate::context::{new_container, relayout_if_dirty};
        use crate::styled_container::StyledContainer;
        use layout_core::AlignItems;
        use renderer_core::RectStyle;

        reset_layout_runtime();
        let built: Rc<RefCell<Vec<NodeId>>> = Rc::new(RefCell::new(Vec::new()));
        let sink = built.clone();
        let ids = signal(Vec::<i32>::new());
        let src = ids.clone();

        let row = Container::from_slots(
            LayoutStyle::new()
                .flex_row()
                .align_items(AlignItems::STRETCH),
            vec![fragment_gap(
                move || src.get(),
                |id: &i32| *id as u64,
                move |_id| {
                    let sink = sink.clone();
                    let chip = StyledContainer::new(
                        LayoutStyle::new().flex_column().padding_horizontal(10.0),
                        move |_| RectStyle::default(),
                        vec![Box::new(Container::new(
                            LayoutStyle::new().width(10.0).height(13.0),
                            vec![],
                        )?)],
                    )?;
                    let node = chip.layout_node();
                    sink.borrow_mut().push(node);
                    Ok(Box::new(chip) as Box<dyn LayoutItem>)
                },
                8.0,
            )],
        )
        .unwrap();

        let root = new_container(
            LayoutStyle::new()
                .flex_row()
                .align_items(AlignItems::STRETCH)
                .width(200.0)
                .height(34.0),
            &[row.layout_node()],
        )
        .unwrap();
        compute_layout(
            root,
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(34.0),
        )
        .unwrap();

        ids.set(vec![1, 2, 3]);
        relayout_if_dirty();

        let chips = built.borrow().clone();
        assert_eq!(chips.len(), 3, "three chips built");
        for node in chips {
            let h = track_layout(node).unwrap().get().height;
            assert!(
                (h - 34.0).abs() < 0.01,
                "each chip must stretch to the 34px zone height, got {h} (content height ~13 means a \
                 collapsing wrapper crept back in)"
            );
        }
    }

    // `for … gap:N` inside a `row` stays transparent AND spaced: the items flow horizontally, `gap` apart,
    // carried as a per-item leading margin (no box of its own). 10px leaves + 8px gap → x 0, 18, 36.
    #[test]
    fn fragment_gap_spaces_items_along_the_row() {
        reset_layout_runtime();
        let built: Rc<RefCell<Vec<NodeId>>> = Rc::new(RefCell::new(Vec::new()));
        let sink = built.clone();
        let items = signal(vec![1u32, 2, 3]);
        let src = items.clone();
        let container = Container::from_slots(
            LayoutStyle::new().flex_row(),
            vec![fragment_gap(
                move || src.get(),
                |n: &u32| *n,
                move |_n| {
                    let (node, item) = leaf10_node();
                    sink.borrow_mut().push(node);
                    Ok(item)
                },
                8.0,
            )],
        )
        .unwrap();
        compute_layout(
            container.layout_node(),
            AvailableSpace::Definite(500.0),
            AvailableSpace::Definite(50.0),
        )
        .unwrap();

        let frag = built.borrow().clone();
        assert_eq!(frag.len(), 3);
        let x = |node: NodeId| track_layout(node).unwrap().get().x;
        assert!(x(frag[0]).abs() < 0.01, "first item flush: {}", x(frag[0]));
        assert!(
            (x(frag[1]) - 18.0).abs() < 0.01,
            "10px item + 8px gap → 18: {}",
            x(frag[1])
        );
        assert!(
            (x(frag[2]) - 36.0).abs() < 0.01,
            "two 10px items + two 8px gaps → 36: {}",
            x(frag[2])
        );
    }

    // The gap margin is re-applied per reconcile, not baked at build: when a keyed item moves to the front it
    // must LOSE its leading margin (else it would be pushed off by a stale gap), and the former first gains one.
    #[test]
    fn fragment_gap_reorder_moves_gap_off_the_new_first_item() {
        reset_layout_runtime();
        let built: Rc<RefCell<Vec<NodeId>>> = Rc::new(RefCell::new(Vec::new()));
        let sink = built.clone();
        let items = signal(vec![1u32, 2, 3]);
        let src = items.clone();
        let container = Container::from_slots(
            LayoutStyle::new().flex_row(),
            vec![fragment_gap(
                move || src.get(),
                |n: &u32| *n,
                move |_n| {
                    let (node, item) = leaf10_node();
                    sink.borrow_mut().push(node);
                    Ok(item)
                },
                8.0,
            )],
        )
        .unwrap();
        let root = container.layout_node();
        let space = || {
            (
                AvailableSpace::Definite(500.0),
                AvailableSpace::Definite(50.0),
            )
        };
        compute_layout(root, space().0, space().1).unwrap();

        // Initial builds, in source order: [key1, key2, key3]. No reorder rebuilds a persisting key, so these
        // node ids stay valid across the reconcile below.
        let (n1, n3) = {
            let b = built.borrow();
            (b[0], b[2])
        };
        let x = |node: NodeId| track_layout(node).unwrap().get().x;
        assert!(x(n1).abs() < 0.01, "key1 first, flush: {}", x(n1));
        assert!((x(n3) - 36.0).abs() < 0.01, "key3 last, at 36: {}", x(n3));

        // key3 moves to the front: it drops its gap margin (→ 0), key1 becomes second (→ 18).
        items.set(vec![3, 1, 2]);
        compute_layout(root, space().0, space().1).unwrap();
        assert!(
            x(n3).abs() < 0.01,
            "reordered-to-front item drops its gap margin: {}",
            x(n3)
        );
        assert!(
            (x(n1) - 18.0).abs() < 0.01,
            "former-first item now second, at 18: {}",
            x(n1)
        );
    }
}
