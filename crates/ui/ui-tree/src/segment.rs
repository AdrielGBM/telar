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

use reactive_core::{Effect, RwSignal, create_effect, create_rw_signal};
use renderer_core::DrawCommand;

use crate::component::Component;
use crate::render_node::RenderNode;

thread_local! {
    // Subscribed by every segment. Bumped after each event so segments re-run even when their view
    // reads signals the effect cannot auto-track — notably the binary-side root segment under hot
    // reload, whose view reads signals created in the app dylib (cross-boundary tracking is
    // unreliable, so the force-tick makes it re-read the current values, e.g. the real viewport).
    static FORCE_TICK: RwSignal<u64> = create_rw_signal(0);
}

/// Forces every segment subscribed to `FORCE_TICK` to re-run on the next flush.
pub fn bump_force_ticks() {
    FORCE_TICK.with(|s| s.set(s.peek().wrapping_add(1)));
}

pub struct Segment {
    // This component's own flattened commands, excluding children (spliced in at compose time).
    own: Rc<RefCell<Vec<DrawCommand>>>,
    // (index into `own` where the child's commands splice, child segment), in emission order.
    child_slots: Rc<RefCell<Vec<(usize, Rc<Segment>)>>>,
    // Set by the effect when this segment's output changes; cleared when composed. This lives on the
    // Segment object (not a thread-local) so it works across the hot-reload dylib boundary: a dylib
    // segment's effect sets it and the binary's compose/dirty-check read the same `Cell` — whereas a
    // thread-local generation would be a separate duplicated instance per side.
    dirty: Rc<Cell<bool>>,
    _effect: Effect,
}

impl Segment {
    /// Mounts `component` as a reactive segment with its own effect: the effect re-runs (and bumps
    /// the thread-local render generation) only when a signal read by this component's `view()`
    /// changes — so a leaf's signal change costs O(this component), not O(tree).
    pub fn mount<C: Component + 'static>(component: C) -> Rc<Segment> {
        let component = Rc::new(RefCell::new(component));
        Self::mount_fn(move || component.try_borrow().ok().map(|c| c.view()))
    }

    /// As `mount`, but takes an already-shared component so a parent can also hold it for event
    /// dispatch. The view path borrows it immutably; events borrow it mutably. These normally never
    /// overlap (dispatch is batched, so flushes happen after it), but a re-entrant flush during
    /// dispatch would otherwise panic, so the render is skipped when the component is borrowed.
    pub fn mount_dyn(component: Rc<RefCell<dyn Component>>) -> Rc<Segment> {
        Self::mount_fn(move || component.try_borrow().ok().map(|c| c.view()))
    }

    /// Core mount: `render` produces this segment's `RenderNode`, or `None` to keep the previous
    /// render unchanged (used when the underlying widget is mid event-dispatch and cannot be
    /// borrowed — borrowing it then would panic, so we leave the last frame's commands in place and
    /// a later flush re-runs us).
    pub fn mount_fn(render: impl Fn() -> Option<RenderNode> + 'static) -> Rc<Segment> {
        let own: Rc<RefCell<Vec<DrawCommand>>> = Default::default();
        let child_slots: Rc<RefCell<Vec<(usize, Rc<Segment>)>>> = Default::default();
        let stack: Rc<RefCell<Vec<RenderNode>>> = Default::default();
        // Starts dirty so the first compose includes this segment.
        let dirty = Rc::new(Cell::new(true));

        let own_c = Rc::clone(&own);
        let slots_c = Rc::clone(&child_slots);
        let dirty_c = Rc::clone(&dirty);
        let _effect = create_effect(move || {
            FORCE_TICK.with(|s| s.get()); // re-run on force-tick (cross-boundary inputs / hot reload)
            let Some(node) = render() else {
                return; // widget is mutably borrowed (event dispatch); keep last render
            };
            let mut own = own_c.borrow_mut();
            let mut stk = stack.borrow_mut();
            let mut new_slots: Vec<(usize, Rc<Segment>)> = Vec::new();
            let own_changed = flatten_segment(node, &mut own, &mut new_slots, &mut stk);
            drop(stk);
            drop(own);
            let mut slots = slots_c.borrow_mut();
            // The boundary structure also changes the output, even if own commands are identical.
            let slots_changed = slots.len() != new_slots.len()
                || slots
                    .iter()
                    .zip(new_slots.iter())
                    .any(|(a, b)| a.0 != b.0 || !Rc::ptr_eq(&a.1, &b.1));
            if own_changed || slots_changed {
                *slots = new_slots;
                dirty_c.set(true);
            }
        });

        Rc::new(Segment {
            own,
            child_slots,
            dirty,
            _effect,
        })
    }

    /// A reference to this segment for a parent's `view()`. Cheap: clones an `Rc`, does not flatten.
    pub fn boundary(self: &Rc<Self>) -> RenderNode {
        RenderNode::Boundary {
            child: Rc::clone(self),
        }
    }
}

/// Like `flatten_view`, but `RenderNode::Boundary` records a child-splice point instead of emitting
/// the child's commands. Returns whether the own command list changed in place.
fn flatten_segment(
    root: RenderNode,
    out: &mut Vec<DrawCommand>,
    slots: &mut Vec<(usize, Rc<Segment>)>,
    stack: &mut Vec<RenderNode>,
) -> bool {
    stack.clear();
    stack.push(root);
    let mut pos: usize = 0;
    let mut changed = false;

    macro_rules! emit_cmd {
        ($cmd:expr) => {{
            let cmd = $cmd;
            if pos < out.len() {
                if out[pos] != cmd {
                    out[pos] = cmd;
                    changed = true;
                }
            } else {
                out.push(cmd);
                changed = true;
            }
            pos += 1;
        }};
    }

    while let Some(node) = stack.pop() {
        match node {
            RenderNode::Empty => {}
            RenderNode::Primitive(cmd) => emit_cmd!(cmd),
            RenderNode::Group { children } => {
                for child in children.into_iter().rev() {
                    stack.push(child);
                }
            }
            RenderNode::Transform { matrix, children } => {
                stack.push(RenderNode::Primitive(DrawCommand::PopMatrix));
                for child in children.into_iter().rev() {
                    stack.push(child);
                }
                emit_cmd!(DrawCommand::PushMatrix { matrix });
            }
            RenderNode::Clip {
                rect,
                radius,
                children,
            } => {
                stack.push(RenderNode::Primitive(DrawCommand::PopClip));
                for child in children.into_iter().rev() {
                    stack.push(child);
                }
                emit_cmd!(DrawCommand::PushClip { rect, radius });
            }
            RenderNode::Layer {
                opacity,
                backdrop_blur,
                children,
            } => {
                stack.push(RenderNode::Primitive(DrawCommand::PopLayer));
                for child in children.into_iter().rev() {
                    stack.push(child);
                }
                emit_cmd!(DrawCommand::PushLayer {
                    opacity,
                    backdrop_blur
                });
            }
            // The child's commands are owned by its own segment; record where they splice in.
            RenderNode::Boundary { child } => slots.push((pos, child)),
        }
    }

    if pos != out.len() {
        out.truncate(pos);
        changed = true;
    }
    changed
}

/// Lazily composes a segment subtree into a flat command list, splicing each child's current
/// commands at its recorded position. O(total commands) but only cheap clones — the expensive
/// `view()` + flatten already ran (per segment) and is skipped for unchanged segments.
pub(crate) fn compose_into(seg: &Segment, out: &mut Vec<DrawCommand>) {
    seg.dirty.set(false);
    let own = seg.own.borrow();
    let slots = seg.child_slots.borrow();
    let mut si = 0;
    for (i, cmd) in own.iter().enumerate() {
        while si < slots.len() && slots[si].0 == i {
            compose_into(&slots[si].1, out);
            si += 1;
        }
        out.push(cmd.clone());
    }
    while si < slots.len() {
        compose_into(&slots[si].1, out);
        si += 1;
    }
}

/// Whether any segment in the subtree has changed since the last compose. O(segments) — cheaper than
/// a full O(commands) recompose, so it gates whether a recompose is needed.
fn any_dirty(seg: &Segment) -> bool {
    if seg.dirty.get() {
        return true;
    }
    seg.child_slots
        .borrow()
        .iter()
        .any(|(_, child)| any_dirty(child))
}

/// Top-level holder for a segment tree (analog of `ComponentList`): exposes the composed commands.
/// Change detection uses per-segment dirty flags (shared across the hot-reload boundary) rather than
/// a thread-local generation, which would be duplicated per side.
pub struct SegmentRoot {
    root: Rc<Segment>,
    cached: RefCell<Vec<DrawCommand>>,
    // Bumped each time the composed output is rebuilt; consumers use it for an O(1) "did content change" compare (e.g. the HW idle-blit).
    compose_gen: Cell<u64>,
    valid: Cell<bool>,
}

impl SegmentRoot {
    pub fn mount<C: Component + 'static>(component: C) -> Self {
        Self::from_segment(Segment::mount(component))
    }

    pub fn from_segment(root: Rc<Segment>) -> Self {
        SegmentRoot {
            root,
            cached: RefCell::new(Vec::new()),
            compose_gen: Cell::new(0),
            valid: Cell::new(false),
        }
    }

    pub fn generation(&self) -> u64 {
        self.compose_gen.get()
    }

    /// Whether any segment changed since the last `commands()` (which clears the dirty flags).
    pub fn is_dirty(&self) -> bool {
        !self.valid.get() || any_dirty(&self.root)
    }

    pub fn commands(&self) -> Ref<'_, Vec<DrawCommand>> {
        if !self.valid.get() || any_dirty(&self.root) {
            let mut cached = self.cached.borrow_mut();
            cached.clear();
            compose_into(&self.root, &mut cached); // clears dirty flags as it walks
            drop(cached);
            self.compose_gen.set(self.compose_gen.get().wrapping_add(1));
            self.valid.set(true);
        }
        self.cached.borrow()
    }
}

#[cfg(test)]
mod tests {
    use geometry_core::Rect;
    use reactive_core::{RwSignal, create_rw_signal};
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
        let a = create_rw_signal(0.0f32);
        let b = create_rw_signal(100.0f32);
        let (sa, sb) = (a.clone(), b.clone());
        let children = vec![Segment::mount(Leaf { x: sa }), Segment::mount(Leaf { x: sb })];
        let root = SegmentRoot::mount(Parent { children });
        // 2 children × 2 rects each.
        assert_eq!(root.commands().len(), 4);
    }

    #[test]
    fn child_change_updates_output_without_parent_rerun() {
        let a = create_rw_signal(0.0f32);
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
}
