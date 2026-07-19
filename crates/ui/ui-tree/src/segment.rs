//! Fine-grained reactive segments (T-1.1 / F010).
//!
//! Today the whole app is one effect that re-runs `app.root().view()` — recursing every component —
//! on any tracked signal, so a single hover/animation costs O(tree). A `Segment` instead mounts a
//! component with its OWN effect that flattens only that component's `view()` into its own command
//! buffer. A parent references a child via `RenderNode::Boundary` (a cheap `Rc` clone) instead of
//! calling `child.view()`, so the parent's effect never re-runs the child, and a child's signal
//! change re-runs only the child. The flat command list is composed lazily at collect time.

use std::cell::{Cell, Ref, RefCell};
use std::rc::Rc;

use geometry_core::Rect;
use reactive_core::{Effect, RwSignal, effect, signal};
use renderer_core::DrawCommand;

use crate::component::Component;
use crate::render_node::RenderNode;

reactive_core::surface_local! {
    /// A per-surface force-tick signal, subscribed by every segment on that surface. Bumped after each event
    /// so segments re-run even when their view reads signals the effect cannot auto-track — notably the
    /// binary-side root segment under hot reload, whose view reads signals created in the app dylib
    /// (cross-boundary tracking is unreliable, so the force-tick makes it re-read current values, e.g. the
    /// real viewport). Per-surface so one surface's event does not force-render the others; a global change
    /// (theme) re-renders all surfaces via its own shared signal, not this tick.
    slot FORCE_TICK: RwSignal<u64> = signal(0);
    access with_force_tick, with_force_tick_ref;
    context ForceTickContext, ForceTickGuard;
}

/// The active surface's force-tick signal (cloned out of the slot so callers never hold the slot borrow
/// across a `.set()`, whose flush would re-enter the slot to read the tick).
fn force_tick() -> RwSignal<u64> {
    with_force_tick_ref(|s| s.clone())
}

/// Forces every segment subscribed to the active surface's `FORCE_TICK` to re-run on the next flush.
pub fn bump_force_ticks() {
    let tick = force_tick();
    tick.set(tick.peek().wrapping_add(1));
}

/// (index into `own` where the child's commands splice, child segment, whether inside an `Overlay`).
type ChildSlots = Vec<(usize, Rc<Segment>, bool)>;

/// One entry on the flatten work stack: a node to process, or a marker that closes the current overlay
/// region (pushed after an `Overlay`'s children so the region's end position is recorded once they are all
/// flattened). Kept private to the flatten walk. The `Node` variant dwarfs `EndOverlay`, but boxing it
/// would add an allocation on the hot flatten path for no real memory win (the stack is short-lived).
#[allow(clippy::large_enum_variant)]
enum Step {
    Node(RenderNode),
    EndOverlay,
}

pub struct Segment {
    // Human-readable widget type name, captured at mount for the devtools tree inspector.
    name: &'static str,
    // This component's own flattened commands, excluding children (spliced in at compose time).
    own_commands: Rc<RefCell<Vec<DrawCommand>>>,
    // Parallel to `own_commands`: whether each command belongs to an `Overlay` region (hoisted to the top
    // layer at compose time). Same length as `own_commands`.
    own_overlay: Rc<RefCell<Vec<bool>>>,
    // Child splice points in emission order (see [`ChildSlots`]).
    child_slots: Rc<RefCell<ChildSlots>>,
    // Set by the effect when this segment's output changes; cleared when composed. This lives on the Segment object (not a thread-local) so it works across the hot-reload dylib boundary: a dylib segment's effect sets it and the binary's compose/dirty-check read the same `Cell` — whereas a thread-local generation would be a separate duplicated instance per side.
    is_dirty: Rc<Cell<bool>>,
    _effect: Effect,
}

/// A node emitted by [`Segment::walk`]: one mounted component, with its pre-order id, widget name,
/// nesting depth, and the bounding rect of its own draw commands unioned with all descendants'.
#[derive(Clone, Debug)]
pub struct SegmentNodeInfo {
    pub id: u64,
    pub name: &'static str,
    pub depth: usize,
    pub rect: Rect,
}

/// Unions two rects, treating any zero/negative-area rect as empty so `empty ∪ r == r` — a leaf with
/// no draw commands must not drag its parent's box to the origin.
fn union_nonempty(a: Rect, b: Rect) -> Rect {
    let a_empty = a.width <= 0.0 || a.height <= 0.0;
    let b_empty = b.width <= 0.0 || b.height <= 0.0;
    match (a_empty, b_empty) {
        (true, _) => b,
        (_, true) => a,
        _ => a.union(b),
    }
}

impl Segment {
    /// Mounts `component` as a reactive segment with its own effect: the effect re-runs (and bumps
    /// the thread-local render generation) only when a signal read by this component's `view()`
    /// changes — so a leaf's signal change costs O(this component), not O(tree).
    pub fn mount<C: Component + 'static>(component: C) -> Rc<Segment> {
        let name = component.debug_name();
        let component = Rc::new(RefCell::new(component));
        Self::mount_fn_named(name, move || component.try_borrow().ok().map(|c| c.view()))
    }

    /// As `mount`, but takes an already-shared component so a parent can also hold it for event
    /// dispatch. The view path borrows it immutably; events borrow it mutably. These normally never
    /// overlap (dispatch is batched, so flushes happen after it), but a re-entrant flush during
    /// dispatch would otherwise panic, so the render is skipped when the component is borrowed.
    pub fn mount_dyn(component: Rc<RefCell<dyn Component>>) -> Rc<Segment> {
        let name = component
            .try_borrow()
            .map(|c| c.debug_name())
            .unwrap_or("Component");
        Self::mount_fn_named(name, move || component.try_borrow().ok().map(|c| c.view()))
    }

    /// Core mount: `render` produces this segment's `RenderNode`, or `None` to keep the previous
    /// render unchanged (used when the underlying widget is mid event-dispatch and cannot be
    /// borrowed — borrowing it then would panic, so we leave the last frame's commands in place and
    /// a later flush re-runs us).
    pub fn mount_fn(render: impl Fn() -> Option<RenderNode> + 'static) -> Rc<Segment> {
        Self::mount_fn_named("Component", render)
    }

    /// As `mount_fn`, but records a human-readable widget `name` for the devtools tree inspector.
    pub fn mount_fn_named(
        name: &'static str,
        render: impl Fn() -> Option<RenderNode> + 'static,
    ) -> Rc<Segment> {
        let own_commands: Rc<RefCell<Vec<DrawCommand>>> = Default::default();
        let own_overlay: Rc<RefCell<Vec<bool>>> = Default::default();
        let child_slots: Rc<RefCell<ChildSlots>> = Default::default();
        let stack: Rc<RefCell<Vec<Step>>> = Default::default();
        // Starts dirty so the first compose includes this segment.
        let is_dirty = Rc::new(Cell::new(true));

        let own_c = Rc::clone(&own_commands);
        let overlay_c = Rc::clone(&own_overlay);
        let slots_c = Rc::clone(&child_slots);
        let dirty_c = Rc::clone(&is_dirty);
        let _effect = effect(move || {
            force_tick().get(); // re-run on force-tick (cross-boundary inputs / hot reload)
            let Some(node) = render() else {
                return; // widget is mutably borrowed (event dispatch); keep last render
            };
            let mut own = own_c.borrow_mut();
            let mut overlay = overlay_c.borrow_mut();
            let mut stk = stack.borrow_mut();
            let mut new_slots: ChildSlots = Vec::new();
            let own_changed =
                flatten_segment(node, &mut own, &mut overlay, &mut new_slots, &mut stk);
            drop(stk);
            drop(own);
            drop(overlay);
            let mut slots = slots_c.borrow_mut();
            // The boundary structure also changes the output, even if own commands are identical.
            let slots_changed = slots.len() != new_slots.len()
                || slots
                    .iter()
                    .zip(new_slots.iter())
                    .any(|(a, b)| a.0 != b.0 || a.2 != b.2 || !Rc::ptr_eq(&a.1, &b.1));
            if own_changed || slots_changed {
                *slots = new_slots;
                dirty_c.set(true);
            }
        });

        Rc::new(Segment {
            name,
            own_commands,
            own_overlay,
            child_slots,
            is_dirty,
            _effect,
        })
    }

    /// A reference to this segment for a parent's `view()`. Cheap: clones an `Rc`, does not flatten.
    pub fn boundary(self: &Rc<Self>) -> RenderNode {
        RenderNode::Boundary {
            child: Rc::clone(self),
        }
    }

    /// Human-readable widget type name captured at mount.
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Emits this segment's subtree in pre-order (parent before children) into `out`. See
    /// [`Segment::collect`] for how ids, depth, and bounding rects are computed.
    pub fn walk(&self, out: &mut Vec<SegmentNodeInfo>) {
        self.collect(0, out);
    }

    /// Recursively appends one [`SegmentNodeInfo`] per segment in pre-order. `id` is the pre-order
    /// index, so a consumer can select by both row index and canvas hit-test. Returns this subtree's
    /// bounding rect (own draw commands unioned with all descendants') so a container highlights its
    /// whole subtree, not just its own commands.
    fn collect(&self, depth: usize, out: &mut Vec<SegmentNodeInfo>) -> Rect {
        let idx = out.len();
        // Push before recursing so the parent precedes its children and keeps the pre-order id.
        out.push(SegmentNodeInfo {
            id: idx as u64,
            name: self.name,
            depth,
            rect: Rect::default(),
        });

        let mut bounds = Rect::default();
        for cmd in self.own_commands.borrow().iter() {
            let rect = match cmd {
                DrawCommand::Rect { rect, .. } => *rect,
                DrawCommand::Text { rect, .. } => *rect,
                DrawCommand::Image { rect, .. } => *rect,
                DrawCommand::PushClip { rect, .. } => *rect,
                _ => continue,
            };
            bounds = union_nonempty(bounds, rect);
        }

        for (_, child, _) in self.child_slots.borrow().iter() {
            bounds = union_nonempty(bounds, child.collect(depth + 1, out));
        }

        out[idx].rect = bounds;
        bounds
    }
}

/// Like `flatten_view`, but `RenderNode::Boundary` records a child-splice point instead of emitting
/// the child's commands. Returns whether the own command list changed in place.
fn flatten_segment(
    root: RenderNode,
    out: &mut Vec<DrawCommand>,
    overlay: &mut Vec<bool>,
    slots: &mut ChildSlots,
    stack: &mut Vec<Step>,
) -> bool {
    stack.clear();
    stack.push(Step::Node(root));
    let mut pos: usize = 0;
    let mut changed = false;
    // Nesting depth of `Overlay` regions; > 0 means the commands/children emitted now are hoisted content.
    let mut overlay_depth: usize = 0;
    // Rebuilt fresh each flatten (parallel to `out`), then compared with the stored flags to detect a
    // pure overlay-membership change (same commands, different layering).
    let mut new_overlay: Vec<bool> = Vec::with_capacity(out.len());

    macro_rules! emit_command {
        ($command:expr) => {{
            let command = $command;
            if pos < out.len() {
                if out[pos] != command {
                    out[pos] = command;
                    changed = true;
                }
            } else {
                out.push(command);
                changed = true;
            }
            new_overlay.push(overlay_depth > 0);
            pos += 1;
        }};
    }

    while let Some(step) = stack.pop() {
        let node = match step {
            Step::EndOverlay => {
                overlay_depth -= 1;
                continue;
            }
            Step::Node(node) => node,
        };
        match node {
            RenderNode::Empty => {}
            RenderNode::Primitive(cmd) => emit_command!(cmd),
            RenderNode::Group { children } => {
                for child in children.into_iter().rev() {
                    stack.push(Step::Node(child));
                }
            }
            RenderNode::Transform { matrix, children } => {
                stack.push(Step::Node(RenderNode::Primitive(DrawCommand::PopMatrix)));
                for child in children.into_iter().rev() {
                    stack.push(Step::Node(child));
                }
                emit_command!(DrawCommand::PushMatrix { matrix });
            }
            RenderNode::Clip {
                rect,
                radius,
                children,
            } => {
                stack.push(Step::Node(RenderNode::Primitive(DrawCommand::PopClip)));
                for child in children.into_iter().rev() {
                    stack.push(Step::Node(child));
                }
                emit_command!(DrawCommand::PushClip { rect, radius });
            }
            RenderNode::Layer {
                opacity,
                backdrop_blur,
                children,
            } => {
                stack.push(Step::Node(RenderNode::Primitive(DrawCommand::PopLayer)));
                for child in children.into_iter().rev() {
                    stack.push(Step::Node(child));
                }
                emit_command!(DrawCommand::PushLayer {
                    opacity,
                    backdrop_blur
                });
            }
            // Everything emitted until the matching EndOverlay marker is overlay content (hoisted at compose).
            RenderNode::Overlay { children } => {
                overlay_depth += 1;
                stack.push(Step::EndOverlay);
                for child in children.into_iter().rev() {
                    stack.push(Step::Node(child));
                }
            }
            // The child's commands are owned by its own segment; record where they splice in, plus whether
            // this splice point sits inside an overlay region.
            RenderNode::Boundary { child } => slots.push((pos, child, overlay_depth > 0)),
        }
    }

    if pos != out.len() {
        out.truncate(pos);
        changed = true;
    }
    if *overlay != new_overlay {
        *overlay = new_overlay;
        changed = true;
    }
    changed
}

/// Lazily composes a segment subtree into a flat command list, splicing each child's current
/// commands at its recorded position. O(total commands) but only cheap clones — the expensive
/// `view()` + flatten already ran (per segment) and is skipped for unchanged segments.
/// Composes a segment subtree into `out`, routing any command that belongs to an `Overlay` region into
/// `overlay_out` instead — so overlays land at the end of the final list (drawn on top, free of any
/// ancestor clip/transform). `in_overlay` propagates that state into child segments spliced within an
/// overlay. See [`SegmentRoot::commands`] for the final `out ++ overlay_out` concatenation.
pub(crate) fn compose_into(
    seg: &Segment,
    out: &mut Vec<DrawCommand>,
    overlay_out: &mut Vec<DrawCommand>,
    in_overlay: bool,
) {
    seg.is_dirty.set(false);
    let own_commands = seg.own_commands.borrow();
    let own_overlay = seg.own_overlay.borrow();
    let slots = seg.child_slots.borrow();
    let mut si = 0;
    for (i, cmd) in own_commands.iter().enumerate() {
        while si < slots.len() && slots[si].0 == i {
            compose_into(&slots[si].1, out, overlay_out, in_overlay || slots[si].2);
            si += 1;
        }
        if in_overlay || own_overlay.get(i).copied().unwrap_or(false) {
            overlay_out.push(cmd.clone());
        } else {
            out.push(cmd.clone());
        }
    }
    while si < slots.len() {
        compose_into(&slots[si].1, out, overlay_out, in_overlay || slots[si].2);
        si += 1;
    }
}

/// Whether any segment in the subtree has changed since the last compose. O(segments) — cheaper than
/// a full O(commands) recompose, so it gates whether a recompose is needed.
fn any_dirty(seg: &Segment) -> bool {
    if seg.is_dirty.get() {
        return true;
    }
    seg.child_slots
        .borrow()
        .iter()
        .any(|(_, child, _)| any_dirty(child))
}

/// Top-level holder for a segment tree (analog of `ComponentList`): exposes the composed commands.
/// Change detection uses per-segment dirty flags (shared across the hot-reload boundary) rather than
/// a thread-local generation, which would be duplicated per side.
pub struct SegmentRoot {
    root: Rc<Segment>,
    cached: RefCell<Vec<DrawCommand>>,
    // Bumped each time the composed output is rebuilt; consumers use it for an O(1) "did content change" compare (e.g. the HW idle-blit).
    compose_generation: Cell<u64>,
    cache_valid: Cell<bool>,
}

impl SegmentRoot {
    pub fn mount<C: Component + 'static>(component: C) -> Self {
        Self::from_segment(Segment::mount(component))
    }

    pub fn from_segment(root: Rc<Segment>) -> Self {
        SegmentRoot {
            root,
            cached: RefCell::new(Vec::new()),
            compose_generation: Cell::new(0),
            cache_valid: Cell::new(false),
        }
    }

    pub fn generation(&self) -> u64 {
        self.compose_generation.get()
    }

    /// Emits the whole segment tree in pre-order for the devtools inspector. See [`Segment::walk`].
    pub fn walk(&self, out: &mut Vec<SegmentNodeInfo>) {
        self.root.walk(out);
    }

    /// Whether any segment changed since the last `commands()` (which clears the dirty flags).
    pub fn is_dirty(&self) -> bool {
        !self.cache_valid.get() || any_dirty(&self.root)
    }

    pub fn commands(&self) -> Ref<'_, Vec<DrawCommand>> {
        if !self.cache_valid.get() || any_dirty(&self.root) {
            let mut cached = self.cached.borrow_mut();
            cached.clear();
            // Overlay content is routed aside during compose, then appended so it draws on top of (and
            // outside any clip of) the main tree.
            let mut overlay: Vec<DrawCommand> = Vec::new();
            compose_into(&self.root, &mut cached, &mut overlay, false); // clears dirty flags as it walks
            cached.extend(overlay);
            drop(cached);
            self.compose_generation
                .set(self.compose_generation.get().wrapping_add(1));
            self.cache_valid.set(true);
        }
        self.cached.borrow()
    }
}

#[cfg(test)]
mod tests {
    use geometry_core::Rect;
    use reactive_core::{RwSignal, signal};
    use renderer_core::{Color, RectStyle, ShapeStyle};

    use super::*;

    fn rect(x: f32) -> RenderNode {
        RenderNode::rect(
            Rect::new(x, 0.0, 10.0, 10.0),
            RectStyle::default().with_fill(Color::BLACK),
        )
    }

    struct Leaf {
        x: RwSignal<f32>,
    }
    impl Component for Leaf {
        fn view(&self) -> RenderNode {
            RenderNode::group([rect(self.x.get()), rect(self.x.get() + 5.0)])
        }
    }

    struct Parent {
        children: Vec<Rc<Segment>>,
    }
    impl Component for Parent {
        fn view(&self) -> RenderNode {
            RenderNode::group(self.children.iter().map(|s| s.boundary()))
        }
    }

    struct Nested;
    impl Component for Nested {
        fn view(&self) -> RenderNode {
            RenderNode::group([
                rect(0.0),
                RenderNode::group([rect(1.0), RenderNode::Empty, RenderNode::group([rect(2.0)])]),
                rect(3.0),
            ])
        }
    }

    #[test]
    fn flatten_nested_groups_and_empties() {
        let root = SegmentRoot::mount(Nested);
        // 4 rects; Empty and nested groups contribute nothing structural.
        assert_eq!(root.commands().len(), 4);
    }

    #[test]
    fn composes_children_in_order() {
        let a = signal(0.0f32);
        let b = signal(100.0f32);
        let (sa, sb) = (a.clone(), b.clone());
        let children = vec![
            Segment::mount(Leaf { x: sa }),
            Segment::mount(Leaf { x: sb }),
        ];
        let root = SegmentRoot::mount(Parent { children });
        // 2 children × 2 rects each.
        assert_eq!(root.commands().len(), 4);
    }

    fn cmd_x(c: &DrawCommand) -> f32 {
        match c {
            DrawCommand::Rect { rect, .. } => rect.x,
            _ => -1.0,
        }
    }

    struct WithOverlay;
    impl Component for WithOverlay {
        fn view(&self) -> RenderNode {
            RenderNode::group([rect(1.0), RenderNode::overlay([rect(2.0)]), rect(3.0)])
        }
    }

    #[test]
    fn overlay_hoists_to_end() {
        let root = SegmentRoot::mount(WithOverlay);
        let cmds = root.commands();
        let xs: Vec<f32> = cmds.iter().map(cmd_x).collect();
        // The overlay's rect(2) is emitted between rect(1) and rect(3) but composes last (drawn on top).
        assert_eq!(xs, vec![1.0, 3.0, 2.0]);
    }

    struct OverlayParent {
        child: Rc<Segment>,
    }
    impl Component for OverlayParent {
        fn view(&self) -> RenderNode {
            RenderNode::group([rect(1.0), RenderNode::overlay([self.child.boundary()])])
        }
    }

    #[test]
    fn overlay_hoists_child_segment() {
        // An overlay whose content is a child segment: the child's commands must hoist too.
        let child = Segment::mount(Leaf { x: signal(9.0) }); // emits rect(9), rect(14)
        let root = SegmentRoot::mount(OverlayParent { child });
        let cmds = root.commands();
        let xs: Vec<f32> = cmds.iter().map(cmd_x).collect();
        assert_eq!(xs, vec![1.0, 9.0, 14.0]);
    }

    #[test]
    fn child_change_updates_output_without_parent_rerun() {
        let a = signal(0.0f32);
        let sa = a.clone();
        let children = vec![Segment::mount(Leaf { x: sa })];
        let root = SegmentRoot::mount(Parent { children });
        let g0 = root.generation();
        let first_x = match &root.commands()[0] {
            DrawCommand::Rect { rect, .. } => rect.x,
            _ => unreachable!(),
        };
        assert_eq!(first_x, 0.0);

        a.set(42.0);
        assert_ne!(root.generation(), g0, "child change must bump generation");
        let new_x = match &root.commands()[0] {
            DrawCommand::Rect { rect, .. } => rect.x,
            _ => unreachable!(),
        };
        assert_eq!(new_x, 42.0, "composed output reflects the child update");
    }

    struct MemoLeaf {
        double: reactive_core::Memo<i32>,
    }
    impl Component for MemoLeaf {
        fn view(&self) -> RenderNode {
            rect(self.double.get() as f32)
        }
    }

    #[test]
    fn signal_dependent_segment_updates_with_runner_batching() {
        use reactive_core::{begin_batch, end_batch};
        let a = signal(0.0f32);
        let sa = a.clone();
        let root = SegmentRoot::mount(Leaf { x: sa });
        assert_eq!(animated_rect_x(&root), 0.0);
        begin_batch();
        a.set(42.0);
        end_batch();
        begin_batch();
        let mid = animated_rect_x(&root);
        end_batch();
        assert_eq!(
            mid, 42.0,
            "signal-reading segment must reflect the batched set"
        );
    }

    // Regression probe for the sandbox counter's frozen "Double:" memo: replicates the runner's exact batch bracketing (new_events begin → on_event set → handled end/begin flush → commands → about_to_wait end) around a memo-reading segment.
    #[test]
    fn memo_dependent_segment_updates_with_runner_batching() {
        use reactive_core::{begin_batch, end_batch, memo};
        let count = signal(0i32);
        let count_mv = count.clone();
        let double = memo(move || count_mv.get() * 2);
        let root = SegmentRoot::mount(MemoLeaf {
            double: double.clone(),
        });
        assert_eq!(animated_rect_x(&root), 0.0);

        begin_batch();
        count.set(3);
        end_batch();
        begin_batch();
        let mid = animated_rect_x(&root);
        end_batch();
        assert_eq!(
            mid, 6.0,
            "memo-reading segment must reflect the flushed memo"
        );
    }

    // A widget whose view() color tracks `theme` and whose on_event writes `sel` — like a nav button
    // reading the theme and flipping its own hover/selection state on a pointer event.
    struct ThemedButton {
        theme: RwSignal<f32>,
        sel: RwSignal<i32>,
    }
    impl Component for ThemedButton {
        fn view(&self) -> RenderNode {
            let c = self.theme.get(); // subscribe to theme
            self.sel.get(); // subscribe to sel
            RenderNode::rect(
                Rect::new(0.0, 0.0, 10.0, 10.0),
                RectStyle::default().with_fill(Color::rgba(c, c, c, 1.0)),
            )
        }
        fn on_event(&mut self, _event: &platform_core::Event) -> crate::component::EventResult {
            self.sel.update(|n| *n += 1); // a handler write, like is_hovered/selected
            crate::component::EventResult::Handled
        }
    }

    fn first_rect_r(root: &SegmentRoot) -> f32 {
        match &root.commands()[0] {
            DrawCommand::Rect { style, .. } => style.fill.unwrap().solid_color().r,
            _ => unreachable!(),
        }
    }

    // A segment must keep its reactive subscriptions across event dispatch. The one hard invariant that
    // guarantees it: dispatch must be BATCHED, so a signal written by a handler flushes only after the
    // widget's borrow is released. If dispatch runs UNBATCHED, the write flushes synchronously while the
    // widget is still borrowed — the segment's effect can't borrow it to re-render, skips, and drops its
    // theme subscription (the hot-reload theme-freeze: the app dylib's runtime was never batched). This
    // pins both halves: unbatched loses the subscription; batched preserves it.
    #[test]
    fn dispatch_must_be_batched_or_segment_drops_subscriptions() {
        use reactive_core::{batch, signal};

        // Unbatched dispatch: the handler's write flushes mid-borrow → subscription to `theme` is lost.
        {
            let theme = signal(0.2f32);
            let sel = signal(0i32);
            let widget = Rc::new(RefCell::new(ThemedButton {
                theme: theme.clone(),
                sel: sel.clone(),
            }));
            let render = {
                let w = Rc::clone(&widget);
                move || w.try_borrow().ok().map(|c| c.view())
            };
            let root = SegmentRoot::from_segment(Segment::mount_fn(render));
            assert!((first_rect_r(&root) - 0.2).abs() < 1e-6);

            widget
                .borrow_mut()
                .on_event(&platform_core::Event::CursorLeft); // UNBATCHED write mid-borrow
            theme.set(0.9);
            assert!(
                (first_rect_r(&root) - 0.2).abs() < 1e-6,
                "unbatched dispatch must drop the theme subscription (frozen at old value)"
            );
        }

        // Batched dispatch (what the fix guarantees for the app dylib's runtime): subscription is preserved.
        {
            let theme = signal(0.2f32);
            let sel = signal(0i32);
            let widget = Rc::new(RefCell::new(ThemedButton {
                theme: theme.clone(),
                sel: sel.clone(),
            }));
            let render = {
                let w = Rc::clone(&widget);
                move || w.try_borrow().ok().map(|c| c.view())
            };
            let root = SegmentRoot::from_segment(Segment::mount_fn(render));
            assert!((first_rect_r(&root) - 0.2).abs() < 1e-6);

            batch(|| {
                widget
                    .borrow_mut()
                    .on_event(&platform_core::Event::CursorLeft)
            });
            theme.set(0.9);
            assert!(
                (first_rect_r(&root) - 0.9).abs() < 1e-6,
                "batched dispatch must preserve the theme subscription (tracks new value)"
            );
        }
    }

    struct AnimatedLeaf {
        x: motion_core::Animated<f32>,
    }
    impl Component for AnimatedLeaf {
        fn view(&self) -> RenderNode {
            rect(self.x.get())
        }
    }

    fn animated_rect_x(root: &SegmentRoot) -> f32 {
        match &root.commands()[0] {
            DrawCommand::Rect { rect, .. } => rect.x,
            _ => unreachable!(),
        }
    }

    // T-5.2: a segment reading `Animated::get()` must see the ticker's interpolated value in the
    // SAME `commands()` call once `motion_core::tick` has run — mirroring the runner, which flushes
    // right after tick() so tree.commands() reflects the tick within one frame (docs/animations.md
    // "Ticker integration in the runner"). No sleeps: a fixed base `Instant` advanced by explicit
    // `Duration`s drives the tween deterministically.
    #[test]
    fn animated_get_reflects_tick_in_commands_and_settles() {
        use std::time::{Duration, Instant};

        // Isolate this test's ticker state: the registry is thread-local and other tests on a
        // reused libtest thread must not leak active animations into this one (mirrors the
        // `fresh()` helper in motion-core's own tests).
        motion_core::reset();
        motion_core::set_scale(1.0);

        let anim = motion_core::Animated::new(
            0.0f32,
            motion_core::tween(Duration::from_millis(100), motion_core::Easing::Linear),
        );
        let root = SegmentRoot::mount(AnimatedLeaf { x: anim.clone() });

        // Baseline compose at the resting value.
        assert_eq!(animated_rect_x(&root), 0.0);
        let g0 = root.generation();

        anim.retarget(10.0);
        assert!(
            motion_core::has_active(),
            "retarget must register an active animation"
        );

        let base = Instant::now();
        // First tick only establishes t0 (no dt to integrate yet); nothing should change or recompose.
        motion_core::tick(base);
        assert_eq!(
            root.generation(),
            g0,
            "the t0-establishing tick must not recompose"
        );
        assert_eq!(animated_rect_x(&root), 0.0);

        // Halfway through the tween: commands() must reflect the interpolated value in this same tick.
        // generation() only bumps inside commands()'s lazy recompose, so read the value first and
        // capture the generation right after — capturing it beforehand would still show the stale
        // pre-tick generation and make the `assert_ne!` below vacuous.
        motion_core::tick(base + Duration::from_millis(50));
        let mid_x = animated_rect_x(&root);
        let g1 = root.generation();
        assert!(
            (mid_x - 5.0).abs() < 1e-3,
            "expected the midpoint of the tween, got {mid_x}"
        );
        assert_ne!(g1, g0, "an in-flight tick must bump the compose generation");

        // Full duration: the tween settles and deregisters.
        motion_core::tick(base + Duration::from_millis(100));
        let end_x = animated_rect_x(&root);
        let g2 = root.generation();
        assert_eq!(end_x, 10.0);
        assert_ne!(g2, g1, "the settling tick must still bump the generation");
        assert!(
            !motion_core::has_active(),
            "a settled tween must deregister"
        );

        // An extra tick after settling integrates nothing and must not recompose again.
        motion_core::tick(base + Duration::from_millis(200));
        assert_eq!(animated_rect_x(&root), 10.0);
        assert_eq!(
            root.generation(),
            g2,
            "a tick with no active animations must not bump the generation"
        );
    }
}
