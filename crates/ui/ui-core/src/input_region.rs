//! The per-surface input registry: what takes the pointer, where it is drawn, what lets it through, and which subtrees take no input at all. The input region, hit-testing and focus reachability all read it.

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use geometry_core::{Point, Rect, Transform};
use layout_core::NodeId;
use platform_core::Event;
use reactive_core::ReadSignal;
use ui_tree::EventResult;

/// How a box's own rect takes part in hit-testing, apart from any handler it has.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum InputMode {
    #[default]
    Auto,
    Opaque,
    Transparent,
}

/// One step between where a subtree is laid out and where it is drawn, in the order the drawing applies it.
#[derive(Clone)]
pub(crate) enum Placement {
    Offset(Rc<dyn Fn() -> (f32, f32)>),
    Transform(Rc<dyn Fn() -> Option<[f32; 6]>>),
    Clip(Rc<dyn Fn() -> Rect>),
}

struct Answer {
    token: u64,
    rect: ReadSignal<Rect>,
}

struct Declared {
    node: NodeId,
    token: u64,
    rect: ReadSignal<Rect>,
    mode: InputMode,
}

#[derive(Clone)]
struct Gate {
    token: u64,
    inert: Option<Rc<dyn Fn() -> bool>>,
    shown: Option<Rc<dyn Fn() -> bool>>,
}

/// Where a subtree root belongs when its layout parent says otherwise: scroll content under its viewport, portaled overlay content under the placeholder it was declared at. A hoisted subtree is drawn in surface space, so its geometry stops there.
struct Link {
    token: u64,
    owner: NodeId,
    hoisted: bool,
}

#[derive(Default)]
struct Registry {
    answers: HashMap<NodeId, Answer>,
    declared: Vec<Declared>,
    modes: HashMap<NodeId, InputMode>,
    transparent: usize,
    gates: HashMap<NodeId, Gate>,
    placements: HashMap<NodeId, Vec<(u64, Placement)>>,
    links: HashMap<NodeId, Link>,
}

reactive_core::surface_local! {
    /// Per-surface input registry. The runner activates each surface's [`InputRegionContext`] around its build/event/frame.
    slot INPUT: Registry = Registry::default();
    access with_input, with_input_ref;
    context InputRegionContext, InputRegionGuard;
}

thread_local! {
    static NEXT_TOKEN: Cell<u64> = const { Cell::new(1) };
}

/// Forgets everything registered on the active surface, for a layout runtime that is about to hand the same node ids out again.
pub(crate) fn reset() {
    with_input(|r| *r = Registry::default());
}

/// One widget's registrations, withdrawn together when it drops. Entries carry the handle's token, so a widget outliving a reset cannot withdraw what a newer widget registered under a recycled node id.
pub(crate) struct InputHandle {
    token: u64,
    nodes: Vec<NodeId>,
}

impl InputHandle {
    pub(crate) fn new() -> Self {
        Self {
            token: NEXT_TOKEN.with(|next| next.replace(next.get() + 1)),
            nodes: Vec::new(),
        }
    }

    fn touch(&mut self, node: NodeId) {
        if !self.nodes.contains(&node) {
            self.nodes.push(node);
        }
    }

    /// `node` dispatches pointer input, so its drawn rect joins the input region.
    pub(crate) fn answer(&mut self, node: NodeId, rect: ReadSignal<Rect>) {
        self.touch(node);
        let token = self.token;
        with_input(|r| {
            r.answers.insert(node, Answer { token, rect });
        });
    }

    pub(crate) fn declare(&mut self, node: NodeId, rect: ReadSignal<Rect>, mode: InputMode) {
        self.touch(node);
        let token = self.token;
        with_input(|r| {
            remove_declared(r, node, token);
            if mode != InputMode::Auto {
                r.transparent += usize::from(mode == InputMode::Transparent);
                r.modes.insert(node, mode);
                r.declared.push(Declared {
                    node,
                    token,
                    rect,
                    mode,
                });
            }
        });
    }

    /// `node`'s subtree takes no input while `inert` reads true or `shown` reads false.
    pub(crate) fn gate(
        &mut self,
        node: NodeId,
        inert: Option<Rc<dyn Fn() -> bool>>,
        shown: Option<Rc<dyn Fn() -> bool>>,
    ) {
        self.touch(node);
        let token = self.token;
        with_input(|r| {
            r.gates.insert(
                node,
                Gate {
                    token,
                    inert,
                    shown,
                },
            );
        });
        crate::focus::reach_changed();
    }

    pub(crate) fn place(&mut self, node: NodeId, placement: Placement) {
        self.touch(node);
        let token = self.token;
        with_input(|r| {
            r.placements
                .entry(node)
                .or_default()
                .push((token, placement));
        });
    }

    pub(crate) fn link(&mut self, node: NodeId, owner: NodeId, hoisted: bool) {
        self.touch(node);
        let token = self.token;
        with_input(|r| {
            r.links.insert(
                node,
                Link {
                    token,
                    owner,
                    hoisted,
                },
            );
        });
        crate::focus::reach_changed();
    }
}

impl Drop for InputHandle {
    fn drop(&mut self) {
        if self.nodes.is_empty() {
            return;
        }
        let token = self.token;
        let nodes = std::mem::take(&mut self.nodes);
        with_input(|r| {
            for node in nodes {
                if r.answers.get(&node).is_some_and(|a| a.token == token) {
                    r.answers.remove(&node);
                }
                remove_declared(r, node, token);
                if r.gates.get(&node).is_some_and(|g| g.token == token) {
                    r.gates.remove(&node);
                }
                if let Some(placements) = r.placements.get_mut(&node) {
                    placements.retain(|(owner, _)| *owner != token);
                    if placements.is_empty() {
                        r.placements.remove(&node);
                    }
                }
                if r.links.get(&node).is_some_and(|l| l.token == token) {
                    r.links.remove(&node);
                }
            }
        });
    }
}

fn remove_declared(r: &mut Registry, node: NodeId, token: u64) {
    if let Some(at) = r
        .declared
        .iter()
        .position(|d| d.node == node && d.token == token)
    {
        let removed = r.declared.swap_remove(at);
        r.transparent -= usize::from(removed.mode == InputMode::Transparent);
        r.modes.remove(&node);
    }
}

fn link_of(node: NodeId) -> Option<(NodeId, bool)> {
    with_input_ref(|r| r.links.get(&node).map(|l| (l.owner, l.hoisted)))
}

/// The node `node` belongs to: its owner across a portal or a scroll viewport, its layout parent otherwise.
fn logical_parent(node: NodeId) -> Option<NodeId> {
    link_of(node)
        .map(|(owner, _)| owner)
        .or_else(|| layout_reactive::parent(node))
}

fn geometric_parent(node: NodeId) -> Option<NodeId> {
    match link_of(node) {
        Some((_, true)) => None,
        Some((owner, false)) => Some(owner),
        None => layout_reactive::parent(node),
    }
}

fn mode_of(node: NodeId) -> InputMode {
    with_input_ref(|r| r.modes.get(&node).copied().unwrap_or_default())
}

fn gate_admits(node: NodeId) -> bool {
    let Some(gate) = with_input_ref(|r| r.gates.get(&node).cloned()) else {
        return true;
    };
    !gate.inert.is_some_and(|inert| inert()) && gate.shown.is_none_or(|shown| shown())
}

/// Whether `node` takes input right now: in layout flow, with no gate shut on it or on anything it belongs to.
pub(crate) fn receives_input(node: NodeId) -> bool {
    let mut at = Some(node);
    let mut starts_segment = true;
    while let Some(current) = at {
        if starts_segment && layout_reactive::is_hidden(current) {
            return false;
        }
        if !gate_admits(current) {
            return false;
        }
        let owner = link_of(current).map(|(owner, _)| owner);
        starts_segment = owner.is_some();
        at = owner.or_else(|| layout_reactive::parent(current));
    }
    true
}

/// Whether `node` is `ancestor` or belongs anywhere beneath it, across portals and scroll viewports.
pub(crate) fn is_inside(node: NodeId, ancestor: NodeId) -> bool {
    std::iter::successors(Some(node), |&at| logical_parent(at)).any(|at| at == ancestor)
}

fn placement_at(node: NodeId, index: usize) -> Option<Placement> {
    with_input_ref(|r| {
        r.placements
            .get(&node)
            .and_then(|placements| placements.get(index))
            .map(|(_, placement)| placement.clone())
    })
}

fn bounds(matrix: [f32; 6], rect: Rect) -> Rect {
    let transform = Transform::from_array(matrix);
    let corners = [
        (rect.x, rect.y),
        (rect.x + rect.width, rect.y),
        (rect.x, rect.y + rect.height),
        (rect.x + rect.width, rect.y + rect.height),
    ]
    .map(|(x, y)| transform.apply(Point::new(x, y)));
    let (mut left, mut top, mut right, mut bottom) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for corner in corners {
        left = left.min(corner.x);
        top = top.min(corner.y);
        right = right.max(corner.x);
        bottom = bottom.max(corner.y);
    }
    Rect::new(left, top, right - left, bottom - top)
}

/// `rect`, in `node`'s layout space, moved and cut by `node`'s own placements into the space its parent draws it in. `None` once a clip leaves nothing.
fn place(node: NodeId, rect: Rect, clipped: bool) -> Option<Rect> {
    let mut rect = rect;
    for index in 0.. {
        let Some(placement) = placement_at(node, index) else {
            break;
        };
        rect = match placement {
            Placement::Offset(offset) => {
                let (dx, dy) = offset();
                Rect::new(rect.x + dx, rect.y + dy, rect.width, rect.height)
            }
            Placement::Transform(matrix) => matrix().map_or(rect, |m| bounds(m, rect)),
            Placement::Clip(clip) if clipped => rect.intersect(clip())?,
            Placement::Clip(_) => rect,
        };
    }
    Some(rect)
}

/// A point in the space `node`'s parent draws it in, taken back into `node`'s layout space. `None` when a clip cuts the point away.
fn unplace(node: NodeId, x: f32, y: f32) -> Option<(f32, f32)> {
    let count = with_input_ref(|r| r.placements.get(&node).map_or(0, Vec::len));
    let mut point = (x, y);
    for index in (0..count).rev() {
        let Some(placement) = placement_at(node, index) else {
            continue;
        };
        point = match placement {
            Placement::Offset(offset) => {
                let (dx, dy) = offset();
                (point.0 - dx, point.1 - dy)
            }
            Placement::Transform(matrix) => {
                match matrix().and_then(|m| Transform::from_array(m).invert()) {
                    Some(inverse) => {
                        let local = inverse.apply(Point::new(point.0, point.1));
                        (local.x, local.y)
                    }
                    None => point,
                }
            }
            Placement::Clip(clip) => {
                if !clip().contains(point.0, point.1) {
                    return None;
                }
                point
            }
        };
    }
    Some(point)
}

fn lift(node: NodeId, rect: Rect, clipped: bool) -> Option<Rect> {
    let mut rect = rect;
    let mut at = Some(node);
    while let Some(current) = at {
        rect = place(current, rect, clipped)?;
        at = geometric_parent(current);
    }
    Some(rect)
}

fn drawn(node: NodeId, rect: Rect) -> Option<Rect> {
    lift(node, rect, true).filter(|rect| rect.width > 0.0 && rect.height > 0.0)
}

/// The rect `node` is drawn at on its surface, through scroll offsets, anchoring and render transforms, but not cut by the clips around it — where something anchored to it should sit. Read without subscribing.
pub fn visible_rect(node: NodeId) -> Option<Rect> {
    lift(node, layout_reactive::track_layout(node)?.peek(), false)
}

/// The part of `node` that can be pointed at on its surface: [`visible_rect`] cut by every clip and viewport around it.
pub(crate) fn pointable_rect(node: NodeId) -> Option<Rect> {
    drawn(node, layout_reactive::track_layout(node)?.peek())
}

/// The rects a surface that carves its own input region should take the pointer over: every widget that dispatches pointer input and every `input_opaque` box, where they are drawn, less the `input_transparent` boxes that pierce an opaque one, leaving out whatever takes no input.
pub fn interactive_rects() -> Vec<Rect> {
    let answers: Vec<(NodeId, ReadSignal<Rect>)> =
        with_input_ref(|r| r.answers.iter().map(|(n, a)| (*n, a.rect)).collect());
    let declared: Vec<(NodeId, ReadSignal<Rect>, InputMode)> = with_input_ref(|r| {
        r.declared
            .iter()
            .map(|d| (d.node, d.rect, d.mode))
            .collect()
    });
    let mut region: Vec<Rect> = answers
        .into_iter()
        .filter(|(node, _)| receives_input(*node))
        .filter_map(|(node, rect)| drawn(node, rect.peek()))
        .collect();
    for &(node, rect, mode) in &declared {
        if mode != InputMode::Opaque || !receives_input(node) {
            continue;
        }
        let Some(outer) = drawn(node, rect.peek()) else {
            continue;
        };
        let holes = declared
            .iter()
            .filter(|(hole, _, hole_mode)| {
                *hole_mode == InputMode::Transparent
                    && pierces(*hole, node)
                    && receives_input(*hole)
            })
            .filter_map(|(hole, rect, _)| drawn(*hole, rect.peek()));
        region.extend(holes.fold(vec![outer], |pieces, hole| {
            pieces
                .into_iter()
                .flat_map(|piece| subtract(piece, hole))
                .collect()
        }));
    }
    merge(region)
}

/// Whether a transparent `hole` reaches up to `ancestor` through nothing but declared boxes: the first box that occludes by default stops it.
fn pierces(hole: NodeId, ancestor: NodeId) -> bool {
    let mut at = logical_parent(hole);
    while let Some(current) = at {
        if current == ancestor {
            return true;
        }
        if mode_of(current) == InputMode::Auto {
            return false;
        }
        at = logical_parent(current);
    }
    false
}

/// What the declared boxes directly inside `parent` decide at `(x, y)`, in `parent`'s layout space: the deepest one under the point wins, and opaque wins between siblings. A box that occludes by default is not declared, so nothing beneath it is asked.
fn decision_below(parent: NodeId, x: f32, y: f32) -> Option<InputMode> {
    let mut decision = None;
    for index in 0.. {
        let Some((node, rect, mode)) =
            with_input_ref(|r| r.declared.get(index).map(|d| (d.node, d.rect, d.mode)))
        else {
            break;
        };
        if logical_parent(node) != Some(parent) || !gate_admits(node) {
            continue;
        }
        if !place(node, rect.peek(), true).is_some_and(|drawn| drawn.contains(x, y)) {
            continue;
        }
        let Some((inner_x, inner_y)) = unplace(node, x, y) else {
            continue;
        };
        match decision_below(node, inner_x, inner_y).unwrap_or(mode) {
            InputMode::Opaque => return Some(InputMode::Opaque),
            _ => decision = Some(InputMode::Transparent),
        }
    }
    decision
}

/// Whether a transparent box inside `node` is what `(x, y)`, in `node`'s layout space, lands on.
pub(crate) fn pierced(node: NodeId, x: f32, y: f32) -> bool {
    with_input_ref(|r| r.transparent > 0)
        && decision_below(node, x, y) == Some(InputMode::Transparent)
}

/// Whether `node`, laid out at `layout` and drawn through its own placements, stands between `(x, y)` and the siblings drawn beneath it. `occludes` is only asked for a box that declares nothing.
pub(crate) fn covers(
    node: NodeId,
    layout: Rect,
    occludes: impl FnOnce() -> bool,
    x: f32,
    y: f32,
) -> bool {
    if !place(node, layout, true).is_some_and(|drawn| drawn.contains(x, y)) {
        return false;
    }
    let mode = mode_of(node);
    if mode == InputMode::Auto {
        return occludes();
    }
    if !gate_admits(node) {
        return false;
    }
    if with_input_ref(|r| r.transparent == 0) {
        return mode == InputMode::Opaque;
    }
    let Some((inner_x, inner_y)) = unplace(node, x, y) else {
        return false;
    };
    decision_below(node, inner_x, inner_y).unwrap_or(mode) == InputMode::Opaque
}

fn subtract(from: Rect, hole: Rect) -> Vec<Rect> {
    let Some(cut) = from.intersect(hole) else {
        return vec![from];
    };
    let right = from.x + from.width;
    let bottom = from.y + from.height;
    let cut_right = cut.x + cut.width;
    let cut_bottom = cut.y + cut.height;
    [
        Rect::new(from.x, from.y, from.width, cut.y - from.y),
        Rect::new(from.x, cut_bottom, from.width, bottom - cut_bottom),
        Rect::new(from.x, cut.y, cut.x - from.x, cut.height),
        Rect::new(cut_right, cut.y, right - cut_right, cut.height),
    ]
    .into_iter()
    .filter(|piece| piece.width > 0.0 && piece.height > 0.0)
    .collect()
}

fn encloses(outer: Rect, inner: Rect) -> bool {
    inner.x >= outer.x
        && inner.y >= outer.y
        && inner.x + inner.width <= outer.x + outer.width
        && inner.y + inner.height <= outer.y + outer.height
}

fn joined(a: Rect, b: Rect) -> Option<Rect> {
    if encloses(a, b) {
        return Some(a);
    }
    let stacked =
        a.x == b.x && a.width == b.width && (a.y + a.height == b.y || b.y + b.height == a.y);
    let abreast =
        a.y == b.y && a.height == b.height && (a.x + a.width == b.x || b.x + b.width == a.x);
    (stacked || abreast).then(|| a.union(b))
}

fn merge(mut rects: Vec<Rect>) -> Vec<Rect> {
    'again: loop {
        for i in 0..rects.len() {
            for j in 0..rects.len() {
                if i != j
                    && let Some(joined) = joined(rects[i], rects[j])
                {
                    rects[i] = joined;
                    rects.swap_remove(j);
                    continue 'again;
                }
            }
        }
        return rects;
    }
}

/// Routes `event` into a subtree that takes no input: presses, the wheel and keys stop here, a move arrives as the pointer leaving so a hover held inside settles, and everything else passes.
pub(crate) fn withhold(event: &Event, route: impl FnOnce(&Event) -> EventResult) -> EventResult {
    match event {
        Event::PointerPressed { .. }
        | Event::Scrolled { .. }
        | Event::KeyPressed { .. }
        | Event::KeyReleased { .. } => EventResult::Ignored,
        Event::PointerMoved { .. } => route(&Event::CursorLeft),
        _ => route(event),
    }
}

#[cfg(test)]
#[path = "input_region_test.rs"]
mod tests;
