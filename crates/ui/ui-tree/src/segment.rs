//! Fine-grained reactive segments (T-1.1 / F010).
//!
//! Today the whole app is one effect that re-runs `app.root().view()` — recursing every component — on any tracked signal, so a single hover/animation costs O(tree). A `Segment` instead mounts a component with its OWN effect that flattens only that component's `view()` into its own command buffer. A parent references a child via `RenderNode::Boundary` (a cheap `Rc` clone) instead of calling `child.view()`, so the parent's effect never re-runs the child, and a child's signal change re-runs only the child. The flat command list is composed lazily at collect time.

use std::cell::{Cell, Ref, RefCell};
use std::rc::Rc;

use geometry_core::Rect;
use reactive_core::effect;
use renderer_core::DrawCommand;

use crate::component::Component;
use crate::render_node::RenderNode;

/// (index into `own` where the child's commands splice, child segment, whether inside an `Overlay`).
type ChildSlots = Vec<(usize, Rc<Segment>, bool)>;

/// One entry on the flatten work stack: a node to process, or a marker that closes the current overlay region (pushed after an `Overlay`'s children so the region's end position is recorded once they are all flattened). Kept private to the flatten walk. The `Node` variant dwarfs `EndOverlay`, but boxing it would add an allocation on the hot flatten path for no real memory win (the stack is short-lived).
#[allow(clippy::large_enum_variant)]
enum Step {
    Node(RenderNode),
    EndOverlay,
}

/// One component's reactive boundary: its own flattened commands, the children spliced into them, and the effect that keeps both current.
pub struct Segment {
    // Captured at mount for the devtools tree inspector.
    name: &'static str,
    // This component's own flattened commands, excluding children, each with whether it belongs to an overlay region. One list rather than two of the same length: the flag was compared in a second pass that allocated a `Vec<bool>` per re-render to answer what the per-command comparison already knows.
    own_commands: Rc<RefCell<Vec<(DrawCommand, bool)>>>,
    // Child splice points in emission order.
    child_slots: Rc<RefCell<ChildSlots>>,
    // Set by the effect when this segment's output changes; cleared when composed.
    is_dirty: Rc<Cell<bool>>,
}

/// A node emitted by [`Segment::walk`]: one mounted component, with its pre-order id, widget name, nesting depth, and the bounding rect of its own draw commands unioned with all descendants'.
#[derive(Clone, Debug)]
pub struct SegmentNodeInfo {
    pub id: u64,
    pub name: &'static str,
    pub depth: usize,
    pub rect: Rect,
}

/// Unions two rects, treating any zero/negative-area rect as empty so `empty ∪ r == r` — a leaf with no draw commands must not drag its parent's box to the origin.
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
    /// Mounts `component` as a reactive segment with its own effect: the effect re-runs (and bumps the thread-local render generation) only when a signal read by this component's `view()` changes — so a leaf's signal change costs O(this component), not O(tree).
    pub fn mount<C: Component + 'static>(component: C) -> Rc<Segment> {
        Self::mount_dyn(Rc::new(RefCell::new(component)))
    }

    /// As `mount`, but takes an already-shared component so a parent can also hold it for event dispatch. The view path borrows it immutably; events borrow it mutably. These normally never overlap (dispatch is batched, so flushes happen after it), but a re-entrant flush during dispatch would otherwise panic, so the render is skipped when the component is borrowed.
    pub fn mount_dyn(component: Rc<RefCell<dyn Component>>) -> Rc<Segment> {
        let name = component
            .try_borrow()
            .map(|c| c.debug_name())
            .unwrap_or("Component");
        Self::mount_fn_named(name, move || component.try_borrow().ok().map(|c| c.view()))
    }

    /// Core mount: `render` produces this segment's `RenderNode`, or `None` to keep the previous render unchanged (used when the underlying widget is mid event-dispatch and cannot be borrowed — borrowing it then would panic, so we leave the last frame's commands in place and a later flush re-runs us). `name` is what the devtools tree inspector shows.
    pub fn mount_fn_named(
        name: &'static str,
        render: impl Fn() -> Option<RenderNode> + 'static,
    ) -> Rc<Segment> {
        let own_commands: Rc<RefCell<Vec<(DrawCommand, bool)>>> = Default::default();
        let child_slots: Rc<RefCell<ChildSlots>> = Default::default();
        let stack: Rc<RefCell<Vec<Step>>> = Default::default();
        // Starts dirty, so the first compose includes this segment.
        let is_dirty = Rc::new(Cell::new(true));

        let own_c = Rc::clone(&own_commands);
        let slots_c = Rc::clone(&child_slots);
        let dirty_c = Rc::clone(&is_dirty);
        effect(move || {
            let Some(node) = render() else {
                return; // widget is mutably borrowed (event dispatch); keep last render
            };
            let mut own = own_c.borrow_mut();
            let mut stk = stack.borrow_mut();
            let mut new_slots: ChildSlots = Vec::new();
            let own_changed = flatten_segment(node, &mut own, &mut new_slots, &mut stk);
            drop(stk);
            drop(own);
            let mut slots = slots_c.borrow_mut();
            // The boundary structure also changes the output, even when the commands are identical.
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
            child_slots,
            is_dirty,
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

    /// Emits this segment's subtree in pre-order (parent before children) into `out`. See `Segment::collect` for how ids, depth, and bounding rects are computed.
    pub fn walk(&self, out: &mut Vec<SegmentNodeInfo>) {
        self.collect(0, out);
    }

    /// Recursively appends one [`SegmentNodeInfo`] per segment in pre-order. `id` is the pre-order index, so a consumer can select by both row index and canvas hit-test. Returns this subtree's bounding rect (own draw commands unioned with all descendants') so a container highlights its whole subtree, not just its own commands.
    fn collect(&self, depth: usize, out: &mut Vec<SegmentNodeInfo>) -> Rect {
        let idx = out.len();
        // Pushed before recursing, so the parent precedes its children and keeps the pre-order id.
        out.push(SegmentNodeInfo {
            id: idx as u64,
            name: self.name,
            depth,
            rect: Rect::default(),
        });

        let mut bounds = Rect::default();
        for (cmd, _) in self.own_commands.borrow().iter() {
            // The renderer's own answer to what a command paints, rather than a second list of arms: the local copy knew four commands, so a spanned paragraph contributed nothing and a notification card inspected as an empty rect.
            let Some(rect) = renderer_core::culling::command_visual_rect(
                cmd,
                geometry_core::Transform::IDENTITY.to_array(),
                &renderer_core::culling::FontMetrics::default(),
            ) else {
                continue;
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

/// Flattens one segment's `RenderNode` into its own command list, in place: `RenderNode::Boundary` records a child-splice point instead of emitting the child's commands, which is what keeps a parent's re-render off its children. Returns whether that list changed.
fn flatten_segment(
    root: RenderNode,
    out: &mut Vec<(DrawCommand, bool)>,
    slots: &mut ChildSlots,
    stack: &mut Vec<Step>,
) -> bool {
    stack.clear();
    stack.push(Step::Node(root));
    let mut pos: usize = 0;
    let mut changed = false;
    // Greater than 0 means the commands emitted now are hoisted content.
    let mut overlay_depth: usize = 0;

    macro_rules! emit_command {
        ($command:expr) => {{
            // The layering is compared with the command, so a subtree that moved into an overlay emits the same commands and still changes the output.
            let entry = ($command, overlay_depth > 0);
            if pos < out.len() {
                if out[pos] != entry {
                    out[pos] = entry;
                    changed = true;
                }
            } else {
                out.push(entry);
                changed = true;
            }
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
            // Opens and closes the box in the command stream, so the structure survives flattening the way a clip's does, and a widget that moved to a different parent counts as changed output.
            RenderNode::Element { element, children } => {
                stack.push(Step::Node(RenderNode::Primitive(DrawCommand::PopElement)));
                for child in children.into_iter().rev() {
                    stack.push(Step::Node(child));
                }
                emit_command!(DrawCommand::PushElement { element });
            }
            // Everything until the matching `EndOverlay` marker is overlay content.
            RenderNode::Overlay { children } => {
                overlay_depth += 1;
                stack.push(Step::EndOverlay);
                for child in children.into_iter().rev() {
                    stack.push(Step::Node(child));
                }
            }
            // The child's commands are owned by its own segment, so record where they splice in and whether the splice point sits inside an overlay region.
            RenderNode::Boundary { child } => slots.push((pos, child, overlay_depth > 0)),
        }
    }

    if pos != out.len() {
        out.truncate(pos);
        changed = true;
    }
    changed
}

/// Lazily composes a segment subtree into a flat command list, splicing each child's current commands at its recorded position. O(total commands) but only cheap clones — the expensive `view()` + flatten already ran (per segment) and is skipped for unchanged segments. Composes a segment subtree into `out`, routing any command that belongs to an `Overlay` region into `overlay_out` instead — so overlays land at the end of the final list (drawn on top, free of any ancestor clip/transform). `in_overlay` propagates that state into child segments spliced within an overlay. See [`SegmentRoot::commands`] for the final `out ++ overlay_out` concatenation.
pub(crate) fn compose_into(
    seg: &Segment,
    out: &mut Vec<DrawCommand>,
    overlay_out: &mut Vec<DrawCommand>,
    in_overlay: bool,
) {
    seg.is_dirty.set(false);
    let own_commands = seg.own_commands.borrow();
    let slots = seg.child_slots.borrow();
    let mut si = 0;
    for (i, (cmd, is_overlay)) in own_commands.iter().enumerate() {
        while si < slots.len() && slots[si].0 == i {
            compose_into(&slots[si].1, out, overlay_out, in_overlay || slots[si].2);
            si += 1;
        }
        if in_overlay || *is_overlay {
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

/// Whether any segment in the subtree has changed since the last compose. O(segments) — cheaper than a full O(commands) recompose, so it gates whether a recompose is needed.
fn any_dirty(seg: &Segment) -> bool {
    if seg.is_dirty.get() {
        return true;
    }
    seg.child_slots
        .borrow()
        .iter()
        .any(|(_, child, _)| any_dirty(child))
}

/// Top-level holder for a segment tree (analog of `ComponentList`): exposes the composed commands. Change detection uses per-segment dirty flags (shared across the hot-reload boundary) rather than a thread-local generation, which would be duplicated per side.
pub struct SegmentRoot {
    root: Rc<Segment>,
    cached: RefCell<Vec<DrawCommand>>,
    // Consumers use it for an O(1) "did content change" test.
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
            // Routed aside during compose, then appended so it draws on top of, and outside any clip of, the main tree.
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
#[path = "segment_test.rs"]
mod tests;
