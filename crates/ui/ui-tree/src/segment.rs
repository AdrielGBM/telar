//! Fine-grained reactive segments.
//!
//! Today the whole app is one effect that re-runs `app.root().view()` — recursing every component — on any tracked signal, so a single hover/animation costs O(tree). A `Segment` instead mounts a component with its OWN effect that flattens only that component's `view()` into its own command buffer. A parent references a child via `RenderNode::Boundary` (a cheap `Rc` clone) instead of calling `child.view()`, so the parent's effect never re-runs the child, and a child's signal change re-runs only the child. The flat command list is composed lazily at collect time.

use std::borrow::Cow;
use std::cell::{Cell, OnceCell, Ref, RefCell};
use std::rc::Rc;

use geometry_core::{BoxModel, Insets, Rect, Size, Transform};
use reactive_core::{SurfaceHandle, effect, enter_surface, enter_surface_if_alive};
use renderer_core::DrawCommand;

use crate::component::Component;
use crate::render_node::RenderNode;

/// (index into `own` where the child's commands splice, child segment, the stratum the splice point sits in).
type ChildSlots = Vec<(usize, Rc<Segment>, Stratum)>;

/// Which part of the frame a command is drawn in, bottom to top: the page, the layers fixed over it, and the overlays above both.
///
/// Composed in that order whatever order they were declared in, so a dialog opened from inside the page still covers a bar fixed over it. Within one stratum the order is the document's. A command keeps the highest stratum around it: a tooltip opened from a fixed bar is an overlay, and a layer declared inside an overlay stays one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Stratum {
    #[default]
    Page,
    Fixed,
    Overlay,
}

impl Stratum {
    const COUNT: usize = 3;

    fn index(self) -> usize {
        self as usize
    }
}

/// One entry on the flatten work stack: a node to process, or a marker that closes the current hoisted region (pushed after an `Overlay`'s or a `Fixed`'s children so the region's end position is recorded once they are all flattened). Kept private to the flatten walk. The `Node` variant dwarfs the markers, but boxing it would add an allocation on the hot flatten path for no real memory win (the stack is short-lived).
#[allow(clippy::large_enum_variant)]
enum Step {
    Node(RenderNode),
    EndOverlay,
    EndFixed,
}

/// One component's reactive boundary: its own flattened commands, the children spliced into them, and the effect that keeps both current.
pub struct Segment {
    // Captured at mount for the devtools tree inspector.
    name: &'static str,
    // This component's own flattened commands, excluding children, each with the stratum it is drawn in. One list rather than two of the same length: the stratum was compared in a second pass that allocated a vec per re-render to answer what the per-command comparison already knows.
    own_commands: Rc<RefCell<Vec<(DrawCommand, Stratum)>>>,
    // Child splice points in emission order.
    child_slots: Rc<RefCell<ChildSlots>>,
    // Set by the effect when this segment's output changes; cleared when composed.
    is_dirty: Rc<Cell<bool>>,
    // Set with `is_dirty` when the change moved commands rather than only rewrote them: own commands added, dropped or moved to another stratum, or children spliced elsewhere. Without it, a recompose writes the new commands over the old ones where they stand.
    is_reshaped: Rc<Cell<bool>>,
    // Whether this segment or anything spliced under it changed since it was last composed, linked to the same flag of the segment that splices it.
    stale: Rc<Stale>,
    // How many commands this segment and everything spliced into it put in each stratum the last time it was composed: where the next segment's commands start.
    extent: Cell<[usize; Stratum::COUNT]>,
    /// Set once a [`SegmentRoot`] composes this segment. Wherever such a segment is spliced it goes in as its root's whole frame, so what it hoists stays inside the box it is drawn in: a nested surface's dialog covers that surface, not the window around it.
    composition: OnceCell<Composition>,
}

/// A composition root's frame, kept on the segment so every reader shares one copy: its own [`SegmentRoot`] and any parent that splices it.
#[derive(Default)]
struct Composition {
    commands: RefCell<Vec<DrawCommand>>,
    valid: Cell<bool>,
    generation: Cell<u64>,
    /// How many of `commands` each stratum holds, in the order they are joined.
    lens: Cell<[usize; Stratum::COUNT]>,
    /// The allocation the frame before last was held in, which a patch that moves commands builds the next frame in rather than allocating one the size of the page.
    spare: RefCell<Vec<DrawCommand>>,
    /// Set when `commands` is rebuilt and cleared when a parent splices it. Whichever reader composes first clears the dirty flags, so a parent that did not still has to learn that its copy is stale.
    unspliced: Cell<bool>,
    /// How many commands the parent that last spliced this frame took from it: where the parent's next commands start.
    spliced_len: Cell<usize>,
}

/// A node emitted by [`Segment::walk`]: one mounted component, with its pre-order id, widget name, nesting depth, the bounding rect of its own draw commands unioned with all descendants', and the box model layout resolved for the box it draws.
///
/// The box model is that of the first layout box the component opens, and all zero for a component that opens none. Edges are physical; see [`BoxModel`].
#[derive(Clone, Debug)]
pub struct SegmentNodeInfo {
    pub id: u64,
    /// Borrowed from the binary that mounted the component, and owned once it has crossed from a guest library, whose image may be unloaded before the host is done with it.
    pub name: Cow<'static, str>,
    pub depth: usize,
    pub rect: Rect,
    pub padding: Insets,
    pub margin: Insets,
    pub border: Insets,
    /// `width` is the gap between columns, `height` the gap between rows.
    pub gap: Size,
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
        let own_commands: Rc<RefCell<Vec<(DrawCommand, Stratum)>>> = Default::default();
        let child_slots: Rc<RefCell<ChildSlots>> = Default::default();
        let stack: Rc<RefCell<Vec<Step>>> = Default::default();
        // Starts dirty, so the first compose includes this segment.
        let is_dirty = Rc::new(Cell::new(true));
        let is_reshaped = Rc::new(Cell::new(true));

        let own_c = Rc::clone(&own_commands);
        let slots_c = Rc::clone(&child_slots);
        let dirty_c = Rc::clone(&is_dirty);
        let stale = Rc::new(Stale {
            here: Cell::new(true),
            above: RefCell::default(),
        });
        let stale_c = Rc::clone(&stale);
        let reshaped_c = Rc::clone(&is_reshaped);
        effect(move || {
            let Some(node) = render() else {
                return; // widget is mutably borrowed (event dispatch); keep last render
            };
            let mut own = own_c.borrow_mut();
            let mut stk = stack.borrow_mut();
            let mut new_slots: ChildSlots = Vec::new();
            let flattened = flatten_segment(node, &mut own, &mut new_slots, &mut stk);
            drop(stk);
            drop(own);
            let mut slots = slots_c.borrow_mut();
            // The boundary structure also changes the output, even when the commands are identical.
            let slots_changed = slots.len() != new_slots.len()
                || slots
                    .iter()
                    .zip(new_slots.iter())
                    .any(|(a, b)| a.0 != b.0 || a.2 != b.2 || !Rc::ptr_eq(&a.1, &b.1));
            if flattened.changed || slots_changed {
                *slots = new_slots;
                dirty_c.set(true);
                stale_c.mark();
            }
            if flattened.reshaped || slots_changed {
                reshaped_c.set(true);
            }
        });

        Rc::new(Segment {
            name,
            own_commands,
            child_slots,
            is_dirty,
            stale,
            is_reshaped,
            extent: Cell::new([0; Stratum::COUNT]),
            composition: OnceCell::new(),
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
        self.collect(0, [Transform::IDENTITY; Stratum::COUNT], Stratum::Page, out);
    }

    /// Recursively appends one [`SegmentNodeInfo`] per segment in pre-order. `id` is the pre-order index, so a consumer can select by both row index and canvas hit-test. Returns this subtree's bounding rect (own draw commands unioned with all descendants') so a container highlights its whole subtree, not just its own commands.
    ///
    /// Rects are where the composed frame draws them: `bases` is the matrix each stratum is drawn under where this segment is spliced and `enclosing` the stratum it is spliced in, so a leaf drawn at its own origin and moved into place, a hoisted overlay and a surface nested in a frame all land in the root's coordinates. A nested surface's boxes are looked up in that surface's world, where their ids mean them.
    fn collect(
        &self,
        depth: usize,
        bases: [Transform; Stratum::COUNT],
        enclosing: Stratum,
        out: &mut Vec<SegmentNodeInfo>,
    ) -> Rect {
        let idx = out.len();
        // Pushed before recursing, so the parent precedes its children and keeps the pre-order id.
        out.push(SegmentNodeInfo {
            id: idx as u64,
            name: Cow::Borrowed(self.name),
            depth,
            rect: Rect::default(),
            padding: Insets::default(),
            margin: Insets::default(),
            border: Insets::default(),
            gap: Size::ZERO,
        });

        let mut bounds = Rect::default();
        let mut box_model = None;
        let mut matrices: [Vec<Transform>; Stratum::COUNT] = bases.map(|base| vec![base]);
        let top = |matrices: &[Vec<Transform>; Stratum::COUNT], stratum: Stratum| {
            matrices[stratum.index()]
                .last()
                .copied()
                .unwrap_or(Transform::IDENTITY)
        };
        let mut surfaces = Vec::new();
        let own = self.own_commands.borrow();
        let slots = self.child_slots.borrow();
        let mut next_slot = 0;
        for at in 0..=own.len() {
            while let Some((_, child, stratum)) = slots.get(next_slot).filter(|slot| slot.0 == at) {
                next_slot += 1;
                let stratum = enclosing.max(*stratum);
                let child_bounds = within(&surfaces, || match child.composition.get() {
                    // A composition root is spliced as its whole frame, every stratum of it under the matrix of the place it goes in.
                    Some(_) => child.collect(
                        depth + 1,
                        [top(&matrices, stratum); Stratum::COUNT],
                        Stratum::Page,
                        out,
                    ),
                    None => child.collect(
                        depth + 1,
                        [Stratum::Page, Stratum::Fixed, Stratum::Overlay]
                            .map(|stratum| top(&matrices, stratum)),
                        stratum,
                        out,
                    ),
                });
                bounds = union_nonempty(bounds, child_bounds);
            }
            let Some((cmd, own_stratum)) = own.get(at) else {
                break;
            };
            let stratum = enclosing.max(*own_stratum);
            match cmd {
                DrawCommand::PushMatrix { transform } => {
                    let parent = top(&matrices, stratum);
                    matrices[stratum.index()].push(transform.then(parent));
                }
                DrawCommand::PopMatrix => {
                    let stack = &mut matrices[stratum.index()];
                    if stack.len() > 1 {
                        stack.pop();
                    }
                }
                DrawCommand::PushSurface { surface } => {
                    surfaces.push(Some(SurfaceHandle::from_raw(surface.0)));
                }
                DrawCommand::PopSurface => {
                    surfaces.pop();
                }
                DrawCommand::PushElement { element } => {
                    if box_model.is_none() {
                        box_model = within_world(&surfaces, || {
                            layout_reactive::box_model(layout_reactive::NodeId::from(element.id.0))
                        })
                        .flatten();
                    }
                }
                // The renderer's own answer to what a command paints, rather than a second list of arms: the local copy knew four commands, so a spanned paragraph contributed nothing and a notification card inspected as an empty rect.
                _ => {
                    if let Some(rect) = renderer_core::culling::command_visual_rect(
                        cmd,
                        top(&matrices, stratum),
                        &renderer_core::culling::FontMetrics::default(),
                    ) {
                        bounds = union_nonempty(bounds, rect);
                    }
                }
            }
        }

        let BoxModel {
            padding,
            margin,
            border,
            gap,
        } = box_model.unwrap_or_default();
        let info = &mut out[idx];
        info.rect = bounds;
        info.padding = padding;
        info.margin = margin;
        info.border = border;
        info.gap = gap;
        bounds
    }
}

/// Runs `f` in the world of the innermost surface the walk is inside, or in the one active now outside every surface: entering the innermost alone sets every world it has.
fn within<R>(surfaces: &[Option<SurfaceHandle>], f: impl FnOnce() -> R) -> R {
    match surfaces.last() {
        Some(Some(surface)) => enter_surface(Some(*surface), f),
        Some(None) | None => f(),
    }
}

/// [`within`] for a read of a box by its id, which means nothing outside the world that numbered it: `None` inside a surface with no world on this thread, one torn down or one drawn by a world this process cannot enter.
fn within_world<R>(surfaces: &[Option<SurfaceHandle>], f: impl FnOnce() -> R) -> Option<R> {
    match surfaces.last() {
        Some(Some(surface)) => enter_surface_if_alive(Some(*surface), f),
        Some(None) => None,
        None => Some(f()),
    }
}

/// Flattens one segment's `RenderNode` into its own command list, in place: `RenderNode::Boundary` records a child-splice point instead of emitting the child's commands, which is what keeps a parent's re-render off its children. Returns how that list changed.
fn flatten_segment(
    root: RenderNode,
    out: &mut Vec<(DrawCommand, Stratum)>,
    slots: &mut ChildSlots,
    stack: &mut Vec<Step>,
) -> Flattened {
    stack.clear();
    stack.push(Step::Node(root));
    let mut pos: usize = 0;
    let mut changed = false;
    let mut reshaped = false;
    // Greater than 0 means the commands emitted now are hoisted into that stratum.
    let mut overlay_depth: usize = 0;
    let mut fixed_depth: usize = 0;
    let stratum = |overlay_depth: usize, fixed_depth: usize| match (overlay_depth, fixed_depth) {
        (0, 0) => Stratum::Page,
        (0, _) => Stratum::Fixed,
        _ => Stratum::Overlay,
    };

    macro_rules! emit_command {
        ($command:expr) => {{
            // The layering is compared with the command, so a subtree that moved into an overlay emits the same commands and still changes the output.
            let entry = ($command, stratum(overlay_depth, fixed_depth));
            if pos < out.len() {
                if out[pos] != entry {
                    reshaped |= out[pos].1 != entry.1;
                    out[pos] = entry;
                    changed = true;
                }
            } else {
                out.push(entry);
                changed = true;
                reshaped = true;
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
            Step::EndFixed => {
                fixed_depth -= 1;
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
                emit_command!(DrawCommand::PushMatrix { transform: matrix });
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
                blend,
                mask,
                children,
            } => {
                stack.push(Step::Node(RenderNode::Primitive(DrawCommand::PopLayer)));
                for child in children.into_iter().rev() {
                    stack.push(Step::Node(child));
                }
                emit_command!(DrawCommand::PushLayer {
                    opacity,
                    backdrop_blur,
                    blend,
                    mask
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
            RenderNode::Fixed { children } => {
                fixed_depth += 1;
                stack.push(Step::EndFixed);
                for child in children.into_iter().rev() {
                    stack.push(Step::Node(child));
                }
            }
            // The child's commands are owned by its own segment, so record where they splice in and the stratum the splice point sits in.
            RenderNode::Boundary { child } => {
                slots.push((pos, child, stratum(overlay_depth, fixed_depth)))
            }
        }
    }

    if pos != out.len() {
        out.truncate(pos);
        changed = true;
        reshaped = true;
    }
    Flattened { changed, reshaped }
}

/// How a re-render changed a segment's own commands.
struct Flattened {
    changed: bool,
    /// Commands were added, dropped or moved to another stratum, so they no longer stand where the last compose put them.
    reshaped: bool,
}

/// Composes a segment subtree into one command list per [`Stratum`], splicing each child's current commands at its recorded position. O(commands in the subtree), but only clones: the expensive `view()` and flatten already ran per segment, and are skipped for unchanged ones. Each command goes to the list of its stratum, the highest of its own and `enclosing`, so what is hoisted lands after the page, drawn on top and free of any ancestor clip or transform. Records how many commands each segment put in each list, which is what lets [`patch`] find them again. See [`SegmentRoot::commands`] for how the lists are joined.
pub(crate) fn compose_into(
    seg: &Segment,
    strata: &mut [Vec<DrawCommand>; Stratum::COUNT],
    enclosing: Stratum,
) {
    let before = strata.each_ref().map(Vec::len);
    seg.is_dirty.set(false);
    seg.is_reshaped.set(false);
    let own_commands = seg.own_commands.borrow();
    let slots = seg.child_slots.borrow();
    let mut si = 0;
    for (i, (cmd, own)) in own_commands.iter().enumerate() {
        while si < slots.len() && slots[si].0 == i {
            slots[si].1.stale.hang_under(&seg.stale);
            splice(&slots[si].1, strata, enclosing.max(slots[si].2));
            si += 1;
        }
        strata[enclosing.max(*own).index()].push(cmd.clone());
    }
    while si < slots.len() {
        slots[si].1.stale.hang_under(&seg.stale);
        splice(&slots[si].1, strata, enclosing.max(slots[si].2));
        si += 1;
    }
    seg.extent
        .set(std::array::from_fn(|t| strata[t].len() - before[t]));
    seg.stale.here.set(false);
}

fn splice(child: &Segment, strata: &mut [Vec<DrawCommand>; Stratum::COUNT], at: Stratum) {
    match child.composition.get() {
        Some(composition) => {
            let frame = child.composed();
            strata[at.index()].extend(frame.iter().cloned());
            composition.spliced_len.set(frame.len());
            composition.unspliced.set(false);
        }
        None => compose_into(child, strata, at),
    }
}

/// A composed frame being brought up to date: the frame as it stood, page then fixed layers then overlays, where each stratum started in it, and the runs of it to replace, which are applied together once the walk is done.
struct Patching<'a> {
    commands: &'a mut Vec<DrawCommand>,
    starts: [usize; Stratum::COUNT],
    edits: Vec<Edit>,
}

/// The `old` commands from `at` in the frame as it stood, to be replaced by `new`.
struct Edit {
    at: usize,
    old: usize,
    new: Vec<DrawCommand>,
}

/// Where a patch has got to in each stratum: in the frame as it stood, and in the frame it is making.
#[derive(Clone, Copy, Default)]
struct Cursor {
    old: [usize; Stratum::COUNT],
    new: [usize; Stratum::COUNT],
}

impl Cursor {
    fn step(&mut self, stratum: usize, old: usize, new: usize) {
        self.old[stratum] += old;
        self.new[stratum] += new;
    }
}

impl Patching<'_> {
    fn set(&mut self, stratum: usize, at: usize, command: DrawCommand) {
        self.commands[self.starts[stratum] + at] = command;
    }

    fn replace(&mut self, stratum: usize, at: usize, old: usize, new: Vec<DrawCommand>) {
        if old == 0 && new.is_empty() {
            return;
        }
        self.edits.push(Edit {
            at: self.starts[stratum] + at,
            old,
            new,
        });
    }

    /// Applies every replacement: over the old commands where none changes the length, and otherwise in one pass that moves each command once, however many replacements there are.
    fn apply(self, spare: &mut Vec<DrawCommand>) {
        let Patching {
            commands,
            mut edits,
            ..
        } = self;
        edits.sort_unstable_by_key(|edit| edit.at);
        if edits.iter().all(|edit| edit.old == edit.new.len()) {
            for edit in edits {
                for (offset, command) in edit.new.into_iter().enumerate() {
                    commands[edit.at + offset] = command;
                }
            }
            return;
        }
        spare.clear();
        let mut rest = commands.drain(..);
        let mut taken = 0;
        for edit in edits {
            spare.extend(rest.by_ref().take(edit.at - taken));
            rest.by_ref().take(edit.old).for_each(drop);
            taken = edit.at + edit.old;
            spare.extend(edit.new);
        }
        spare.extend(rest);
        std::mem::swap(commands, spare);
    }
}

/// Brings `seg`'s part of a composed frame up to date from `cursor`, leaving `cursor` past it: a subtree nothing changed in is stepped over, a segment whose commands only changed in value writes them over the old ones where they stand, and only a segment whose commands moved composes its subtree again.
///
/// What keeps a change to one widget on a page of thousands from costing a clone of every command on it. The frame stays one list, as the renderer reads it; a segment that changed length moves what follows it, which is a move and not a clone, and once per patch however many segments did.
fn patch(seg: &Segment, enclosing: Stratum, cursor: &mut Cursor, frame: &mut Patching) {
    let start = cursor.new;
    if seg.is_reshaped.get() {
        let old = seg.extent.get();
        let mut fresh: [Vec<DrawCommand>; Stratum::COUNT] = Default::default();
        compose_into(seg, &mut fresh, enclosing);
        for (t, commands) in fresh.into_iter().enumerate() {
            let len = commands.len();
            frame.replace(t, cursor.old[t], old[t], commands);
            cursor.step(t, old[t], len);
        }
        return;
    }
    if !any_dirty(seg) {
        for (t, len) in seg.extent.get().into_iter().enumerate() {
            cursor.step(t, len, len);
        }
        return;
    }
    let rewrite = seg.is_dirty.replace(false);
    let own_commands = seg.own_commands.borrow();
    let slots = seg.child_slots.borrow();
    let mut si = 0;
    for (i, (cmd, own)) in own_commands.iter().enumerate() {
        while si < slots.len() && slots[si].0 == i {
            slots[si].1.stale.hang_under(&seg.stale);
            patch_child(&slots[si].1, enclosing.max(slots[si].2), cursor, frame);
            si += 1;
        }
        let t = enclosing.max(*own).index();
        if rewrite {
            frame.set(t, cursor.old[t], cmd.clone());
        }
        cursor.step(t, 1, 1);
    }
    while si < slots.len() {
        slots[si].1.stale.hang_under(&seg.stale);
        patch_child(&slots[si].1, enclosing.max(slots[si].2), cursor, frame);
        si += 1;
    }
    seg.extent
        .set(std::array::from_fn(|t| cursor.new[t] - start[t]));
    seg.stale.here.set(false);
}

fn patch_child(child: &Segment, at: Stratum, cursor: &mut Cursor, frame: &mut Patching) {
    let Some(composition) = child.composition.get() else {
        return patch(child, at, cursor, frame);
    };
    let t = at.index();
    let old = composition.spliced_len.get();
    if stale_for_parent(child) {
        let spliced: Vec<DrawCommand> = child.composed().clone();
        let len = spliced.len();
        frame.replace(t, cursor.old[t], old, spliced);
        composition.spliced_len.set(len);
        composition.unspliced.set(false);
        cursor.step(t, old, len);
    } else {
        cursor.step(t, old, old);
    }
}

impl Segment {
    /// This composition root's frame: its page, then its fixed layers, then its overlays. Brought up to date only when something under it changed since the last read, whoever read it, and then only where it changed.
    fn composed(&self) -> Ref<'_, Vec<DrawCommand>> {
        let composition = self
            .composition
            .get()
            .expect("only a composition root has a frame of its own");
        if !composition.valid.get() {
            let mut cached = composition.commands.borrow_mut();
            cached.clear();
            // Hoisted content is routed aside during compose, then appended so it draws on top of, and outside any clip of, the main tree: the fixed layers, then the overlays over them.
            let mut strata: [Vec<DrawCommand>; Stratum::COUNT] = Default::default();
            strata[Stratum::Page.index()] = std::mem::take(&mut *cached);
            compose_into(self, &mut strata, Stratum::Page);
            composition.lens.set(strata.each_ref().map(Vec::len));
            let [page, fixed, overlay] = strata;
            *cached = page;
            cached.extend(fixed);
            cached.extend(overlay);
        } else if any_dirty(self) {
            let lens = composition.lens.get();
            let mut cached = composition.commands.borrow_mut();
            let mut frame = Patching {
                commands: &mut cached,
                starts: [0, lens[0], lens[0] + lens[1]],
                edits: Vec::new(),
            };
            let mut cursor = Cursor::default();
            patch(self, Stratum::Page, &mut cursor, &mut frame);
            frame.apply(&mut composition.spare.borrow_mut());
            composition.lens.set(cursor.new);
        } else {
            return composition.commands.borrow();
        }
        composition
            .generation
            .set(composition.generation.get().wrapping_add(1));
        composition.valid.set(true);
        composition.unspliced.set(true);
        // Whoever splices this frame holds a copy of it, now out of date.
        if let Some(above) = self.stale.above.borrow().clone() {
            above.mark();
        }
        composition.commands.borrow()
    }
}

/// Whether any segment in the subtree has changed since the last compose. One flag, kept by every change marking the segments it is spliced under, so asking costs nothing however large the tree.
fn any_dirty(seg: &Segment) -> bool {
    seg.stale.here.get()
}

/// Whether a segment or anything spliced under it changed since it was last composed.
#[derive(Default)]
struct Stale {
    here: Cell<bool>,
    /// The flag of the segment that last spliced this one, which a change here marks too.
    above: RefCell<Option<Rc<Stale>>>,
}

impl Stale {
    /// Marks this segment and every segment it is spliced under, up to the first already marked: a marked segment's ancestors are all marked until a compose reaches them.
    fn mark(&self) {
        if self.here.replace(true) {
            return;
        }
        let mut above = self.above.borrow().clone();
        while let Some(at) = above {
            if at.here.replace(true) {
                return;
            }
            above = at.above.borrow().clone();
        }
    }

    fn hang_under(&self, parent: &Rc<Stale>) {
        let mut above = self.above.borrow_mut();
        if !above.as_ref().is_some_and(|at| Rc::ptr_eq(at, parent)) {
            *above = Some(Rc::clone(parent));
        }
    }
}

fn stale_for_parent(child: &Segment) -> bool {
    match child.composition.get() {
        Some(composition) => {
            composition.unspliced.get() || !composition.valid.get() || any_dirty(child)
        }
        None => any_dirty(child),
    }
}

/// Top-level holder for a segment tree (analog of `ComponentList`): exposes the composed commands. Change detection uses per-segment dirty flags (shared across the hot-reload boundary) rather than a thread-local generation, which would be duplicated per side.
///
/// Its root becomes a composition root: another tree can splice it through [`boundary`](Self::boundary), and draws it as this root's whole frame, sharing the one composed copy with [`commands`](Self::commands).
pub struct SegmentRoot {
    root: Rc<Segment>,
}

impl SegmentRoot {
    pub fn mount<C: Component + 'static>(component: C) -> Self {
        Self::from_segment(Segment::mount(component))
    }

    pub fn from_segment(root: Rc<Segment>) -> Self {
        root.composition.get_or_init(Composition::default);
        SegmentRoot { root }
    }

    /// Increments whenever the composed commands are rebuilt, whoever asked for them; consumers use it for an O(1) "did content change" test.
    pub fn generation(&self) -> u64 {
        self.composition().generation.get()
    }

    /// Emits the whole segment tree in pre-order for the devtools inspector. See [`Segment::walk`].
    pub fn walk(&self, out: &mut Vec<SegmentNodeInfo>) {
        self.root.walk(out);
    }

    /// Whether any segment changed since the last `commands()` (which clears the dirty flags).
    pub fn is_dirty(&self) -> bool {
        !self.composition().valid.get() || any_dirty(&self.root)
    }

    pub fn commands(&self) -> Ref<'_, Vec<DrawCommand>> {
        self.root.composed()
    }

    /// This tree as a child of another tree's `view()`. The parent draws it as one block in the stratum it sits in — this tree's overlays on top of its own page and under whatever clip the parent put around it — and recomposes when this tree changes.
    pub fn boundary(&self) -> RenderNode {
        self.root.boundary()
    }

    fn composition(&self) -> &Composition {
        self.root
            .composition
            .get()
            .expect("a segment root is a composition root from construction")
    }
}

#[cfg(test)]
#[path = "segment.test.rs"]
mod tests;
