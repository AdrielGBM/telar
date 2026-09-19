use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use geometry_core::{Point, Rect, Transform};
use layout_core::NodeId;
use platform_core::Event;
use reactive_core::ReadSignal;
use ui_tree::EventResult;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum InputMode {
    #[default]
    Auto,
    Opaque,
    Transparent,
}

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

/// A hoisted subtree is drawn in surface space, so its geometry stops there.
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
    /// Several per node, because more than one owner can gate the same node: a box's own `inert`, and a list holding that box mounted while it leaves.
    gates: HashMap<NodeId, Vec<Gate>>,
    placements: HashMap<NodeId, Vec<(u64, Placement)>>,
    links: HashMap<NodeId, Link>,
}

reactive_core::surface_local! {
    /// The runner activates each surface's [`InputRegionContext`] around its build/event/frame.
    slot INPUT: Registry = Registry::default();
    access with_input, with_input_ref;
    context InputRegionContext, InputRegionGuard;
}

thread_local! {
    static NEXT_TOKEN: Cell<u64> = const { Cell::new(1) };
}

/// Node ids can be recycled by the layout runtime, so stale registrations under a reused id must be cleared first.
pub(crate) fn reset() {
    with_input(|r| *r = Registry::default());
}

/// Entries carry the handle's token, so a widget outliving a reset cannot withdraw what a newer widget registered under a recycled node id.
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

    pub(crate) fn gate(
        &mut self,
        node: NodeId,
        inert: Option<Rc<dyn Fn() -> bool>>,
        shown: Option<Rc<dyn Fn() -> bool>>,
    ) {
        self.touch(node);
        let token = self.token;
        with_input(|r| {
            let gates = r.gates.entry(node).or_default();
            gates.retain(|gate| gate.token != token);
            gates.push(Gate {
                token,
                inert,
                shown,
            });
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
                if let Some(gates) = r.gates.get_mut(&node) {
                    gates.retain(|gate| gate.token != token);
                    if gates.is_empty() {
                        r.gates.remove(&node);
                    }
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

#[cfg(test)]
pub(crate) fn mentions(node: NodeId) -> bool {
    with_input_ref(|r| {
        r.answers.contains_key(&node)
            || r.declared.iter().any(|d| d.node == node)
            || r.gates.contains_key(&node)
            || r.placements.contains_key(&node)
            || r.links.contains_key(&node)
    })
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

/// Whether every gate on `node` itself lets input in. Its ancestors are [`receives_input`]'s question.
pub(crate) fn gate_admits(node: NodeId) -> bool {
    let Some(gates) = with_input_ref(|r| r.gates.get(&node).cloned()) else {
        return true;
    };
    gates.iter().all(|gate| {
        !gate.inert.as_ref().is_some_and(|inert| inert())
            && gate.shown.as_ref().is_none_or(|shown| shown())
    })
}

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

/// What a claim is shaped like once the placements above it have been applied.
///
/// Two shapes, because a claim that stayed a rectangle answers for free: an offset, a clip and a translating or scaling transform all keep a box a box, so the pointer path pays nothing for the overwhelming majority that never turned. Rotation and skew are what needs the corners — and an affine transform of a convex shape is convex, as is a clip of one, so the corners stay a convex ring however long the chain above them gets. That is what lets one point test and one decomposition answer for every claim there is.
#[derive(Clone)]
enum Shape {
    Box(Rect),
    Turned(Vec<Point>),
}

impl Shape {
    fn bounds(&self) -> Rect {
        match self {
            Shape::Box(rect) => *rect,
            Shape::Turned(corners) => hull(corners),
        }
    }

    /// Whether the shape has any area to be pointed at. A line is not drawn, however long it is.
    fn is_drawn(&self) -> bool {
        match self {
            Shape::Box(rect) => rect.width > 0.0 && rect.height > 0.0,
            Shape::Turned(corners) => sweep(corners) != 0.0,
        }
    }

    fn contains(&self, x: f32, y: f32) -> bool {
        match self {
            Shape::Box(rect) => rect.contains(x, y),
            Shape::Turned(corners) => encircles(corners, x, y),
        }
    }

    fn offset(self, dx: f32, dy: f32) -> Shape {
        match self {
            Shape::Box(rect) => {
                Shape::Box(Rect::new(rect.x + dx, rect.y + dy, rect.width, rect.height))
            }
            Shape::Turned(corners) => Shape::Turned(
                corners
                    .into_iter()
                    .map(|corner| Point::new(corner.x + dx, corner.y + dy))
                    .collect(),
            ),
        }
    }

    /// An axis-aligned matrix keeps a box a box; one that turns it hands back the corners it draws, since the bounds around those would claim four corners the box is not in.
    fn transformed(self, matrix: [f32; 6]) -> Shape {
        let transform = Transform::from_array(matrix);
        let apply = |corner| transform.apply(corner);
        match self {
            Shape::Box(rect) if matrix[1] == 0.0 && matrix[2] == 0.0 => {
                let start = apply(Point::new(rect.x, rect.y));
                let end = apply(Point::new(rect.x + rect.width, rect.y + rect.height));
                Shape::Box(Rect::new(
                    start.x.min(end.x),
                    start.y.min(end.y),
                    (end.x - start.x).abs(),
                    (end.y - start.y).abs(),
                ))
            }
            Shape::Box(rect) => Shape::Turned(corners_of(rect).map(apply).to_vec()),
            Shape::Turned(corners) => Shape::Turned(corners.into_iter().map(apply).collect()),
        }
    }

    fn clipped(self, clip: Rect) -> Option<Shape> {
        match self {
            Shape::Box(rect) => rect.intersect(clip).map(Shape::Box),
            Shape::Turned(corners) => cut(corners, clip).map(Shape::Turned),
        }
    }

    /// The axis-aligned rects the claim is stated as, which is the only shape a region comes in.
    fn rects(self) -> Vec<Rect> {
        match self {
            Shape::Box(rect) => vec![rect],
            Shape::Turned(corners) => rows(&corners),
        }
    }
}

/// A rect's corners as a ring, clockwise from its top-left.
fn corners_of(rect: Rect) -> [Point; 4] {
    [
        Point::new(rect.x, rect.y),
        Point::new(rect.x + rect.width, rect.y),
        Point::new(rect.x + rect.width, rect.y + rect.height),
        Point::new(rect.x, rect.y + rect.height),
    ]
}

fn edges(corners: &[Point]) -> impl Iterator<Item = (Point, Point)> + '_ {
    corners
        .iter()
        .zip(corners.iter().cycle().skip(1))
        .map(|(&from, &to)| (from, to))
}

fn hull(corners: &[Point]) -> Rect {
    let (mut left, mut top, mut right, mut bottom) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for corner in corners {
        left = left.min(corner.x);
        top = top.min(corner.y);
        right = right.max(corner.x);
        bottom = bottom.max(corner.y);
    }
    Rect::new(left, top, right - left, bottom - top)
}

/// Twice the ring's signed area, whose sign is the winding it was built in.
fn sweep(corners: &[Point]) -> f32 {
    edges(corners)
        .map(|(from, to)| from.x * to.y - to.x * from.y)
        .sum()
}

/// Whether `(x, y)` is inside a convex ring, its boundary included.
///
/// Either winding answers: a transform with a negative determinant — a mirrored box — hands the same corners back in the opposite order.
fn encircles(corners: &[Point], x: f32, y: f32) -> bool {
    let (mut inside, mut outside) = (false, false);
    for (from, to) in edges(corners) {
        let side = (to.x - from.x) * (y - from.y) - (to.y - from.y) * (x - from.x);
        inside |= side > 0.0;
        outside |= side < 0.0;
    }
    !(inside && outside)
}

/// A convex ring cut down to the part of it inside `clip`, or `None` when no part is.
///
/// One pass per side of the clip, keeping the corners on the inside of that side and adding one wherever an edge crosses it. The ring is convex, so each side leaves it convex and the four together leave exactly the intersection.
fn cut(corners: Vec<Point>, clip: Rect) -> Option<Vec<Point>> {
    let sides = [
        (1.0, 0.0, -clip.x),
        (-1.0, 0.0, clip.x + clip.width),
        (0.0, 1.0, -clip.y),
        (0.0, -1.0, clip.y + clip.height),
    ];
    let mut ring = corners;
    for (a, b, c) in sides {
        let depth = |corner: Point| a * corner.x + b * corner.y + c;
        let mut kept = Vec::with_capacity(ring.len() + 1);
        for (from, to) in edges(&ring) {
            let (here, there) = (depth(from), depth(to));
            if here >= 0.0 {
                kept.push(from);
            }
            if (here < 0.0) != (there < 0.0) {
                let crossing = here / (here - there);
                kept.push(Point::new(
                    from.x + (to.x - from.x) * crossing,
                    from.y + (to.y - from.y) * crossing,
                ));
            }
        }
        if kept.len() < 3 {
            return None;
        }
        ring = kept;
    }
    Some(ring)
}

/// The pixel rows a turned claim is stated as.
///
/// A shape with diagonal edges has no exact decomposition into axis-aligned rects, and a row is the unit a compositor rasterises a region in anyway. Each row spans the widest the shape is anywhere inside it, so the claim covers what is drawn rather than sitting inside it: a claim that stopped short of the drawn edge is a click falling through to whatever the surface is over.
fn rows(corners: &[Point]) -> Vec<Rect> {
    let hull = hull(corners);
    let base = hull.y + hull.height;
    let mut rows = Vec::new();
    let mut top = hull.y;
    while top < base {
        let bottom = (top.floor() + 1.0).min(base);
        // Past 2^24 an f32 cannot hold `top + 1`, and a row that does not advance would never end.
        if bottom <= top {
            break;
        }
        if let Some((left, right)) = span(corners, top, bottom) {
            rows.push(Rect::new(left, top, right - left, bottom - top));
        }
        top = bottom;
    }
    rows
}

/// How far left and right the shape reaches anywhere in the band between `top` and `bottom`.
///
/// Its edges are straight, so the extremes sit at a corner inside the band or where an edge crosses one of its two sides; nothing in between reaches further.
fn span(corners: &[Point], top: f32, bottom: f32) -> Option<(f32, f32)> {
    let (mut left, mut right) = (f32::MAX, f32::MIN);
    for (from, to) in edges(corners) {
        if (top..=bottom).contains(&from.y) {
            left = left.min(from.x);
            right = right.max(from.x);
        }
        for side in [top, bottom] {
            if (from.y < side) != (to.y < side) {
                let crossing = (side - from.y) / (to.y - from.y);
                let x = from.x + (to.x - from.x) * crossing;
                left = left.min(x);
                right = right.max(x);
            }
        }
    }
    (right > left).then_some((left, right))
}

fn place(node: NodeId, shape: Shape, clipped: bool) -> Option<Shape> {
    let mut shape = shape;
    for index in 0.. {
        let Some(placement) = placement_at(node, index) else {
            break;
        };
        shape = match placement {
            Placement::Offset(offset) => {
                let (dx, dy) = offset();
                shape.offset(dx, dy)
            }
            Placement::Transform(matrix) => match matrix() {
                Some(matrix) => shape.transformed(matrix),
                None => shape,
            },
            Placement::Clip(clip) if clipped => shape.clipped(clip())?,
            Placement::Clip(_) => shape,
        };
    }
    Some(shape)
}

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
            // A matrix that will not invert flattened the box onto a line, and no point is inside that.
            Placement::Transform(matrix) => match matrix() {
                Some(matrix) => {
                    let inverse = Transform::from_array(matrix).invert()?;
                    let local = inverse.apply(Point::new(point.0, point.1));
                    (local.x, local.y)
                }
                None => point,
            },
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

fn lift(node: NodeId, rect: Rect, clipped: bool) -> Option<Shape> {
    let mut shape = Shape::Box(rect);
    let mut at = Some(node);
    while let Some(current) = at {
        shape = place(current, shape, clipped)?;
        at = geometric_parent(current);
    }
    Some(shape)
}

fn drawn(node: NodeId, rect: Rect) -> Option<Shape> {
    lift(node, rect, true).filter(Shape::is_drawn)
}

pub fn visible_rect(node: NodeId) -> Option<Rect> {
    lift(node, layout_reactive::track_layout(node)?.peek(), false).map(|shape| shape.bounds())
}

/// Whether `(x, y)` lands on `node` where it is drawn, rather than merely inside the bounds around it.
pub(crate) fn pointable(node: NodeId, x: f32, y: f32) -> bool {
    layout_reactive::track_layout(node)
        .and_then(|rect| drawn(node, rect.peek()))
        .is_some_and(|shape| shape.contains(x, y))
}

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
        .flat_map(Shape::rects)
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
            .filter_map(|(hole, rect, _)| drawn(*hole, rect.peek()))
            .flat_map(Shape::rects);
        region.extend(holes.fold(outer.rects(), |pieces, hole| {
            pieces
                .into_iter()
                .flat_map(|piece| subtract(piece, hole))
                .collect()
        }));
    }
    merge(region)
}

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
        if !place(node, Shape::Box(rect.peek()), true).is_some_and(|drawn| drawn.contains(x, y)) {
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

pub(crate) fn pierced(node: NodeId, x: f32, y: f32) -> bool {
    with_input_ref(|r| r.transparent > 0)
        && decision_below(node, x, y) == Some(InputMode::Transparent)
}

pub(crate) fn covers(
    node: NodeId,
    layout: Rect,
    occludes: impl FnOnce() -> bool,
    x: f32,
    y: f32,
) -> bool {
    if !place(node, Shape::Box(layout), true).is_some_and(|drawn| drawn.contains(x, y)) {
        return false;
    }
    if !gate_admits(node) {
        return false;
    }
    let mode = mode_of(node);
    if mode == InputMode::Auto {
        return occludes();
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

/// A move is routed as the pointer leaving, so a hover held inside the withheld subtree settles.
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
