//! [`ReorderGroup`]: items dragged within and between several containers, with a drop preview, drag-out-to-detach and a keyboard path.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use telar::{
    Axis, ConsumedKeys, Container, Cursor, DragAxis, Key, LayoutError, LayoutItem, LayoutStyle,
    NamedKey, Props, Reactive, ReactiveList, Rect, RectStyle, RwSignal, ShapeStyle, SizeDimension,
    StyledContainer, Transaction, box_item, focus::Role, insertion_index, on_cleanup, signal,
    track_layout,
};

use crate::shared;

/// One item's widget, built on demand from its position in the caller's storage.
pub type ItemBuilder = Rc<dyn Fn(usize) -> Result<Box<dyn LayoutItem>, LayoutError>>;

/// An item's place: which container, and where in it. As a drop target, `index` counts slots in that container before the move, the frame [`telar::apply_move`] reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Slot {
    pub zone: usize,
    pub index: usize,
}

impl Slot {
    pub fn new(zone: usize, index: usize) -> Self {
        Self { zone, index }
    }
}

/// How a [`ReorderGroup`] shows where the item in hand would land.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, telar::PreviewArg)]
pub enum DropPreview {
    /// A rule between the two items the drop would land between; nothing else moves.
    #[default]
    Line,
    /// The items make room: the slot the item came out of closes and one of its size opens where it would land, so the container shows the order the drop would leave.
    Gap,
}

/// Where a [`ReorderGroup`] lets an item in hand go.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Reach {
    /// Anywhere the pointer goes: it lands in whichever container is under it, and outside them all it detaches.
    #[default]
    AnyZone,
    /// Along its own container's axis and inside it, landing only there: a single list reordered in place.
    OwnZone,
}

/// Moves the item at `from` into the slot `to` across `zones`, the storage-side twin of a [`ReorderGroup`] move. Returns whether anything moved.
pub fn apply_zone_move<T>(zones: &mut [Vec<T>], from: Slot, to: Slot) -> bool {
    if from.zone == to.zone {
        return zones
            .get_mut(from.zone)
            .is_some_and(|items| telar::apply_move(items, from.index, to.index));
    }
    if to.zone >= zones.len() || from.index >= zones.get(from.zone).map_or(0, Vec::len) {
        return false;
    }
    let item = zones[from.zone].remove(from.index);
    let target = &mut zones[to.zone];
    target.insert(to.index.min(target.len()), item);
    true
}

/// A container's layout and items at the moment the stroke began.
#[derive(Clone)]
struct FrozenZone {
    zone: usize,
    rect: Rect,
    items: Vec<Rect>,
    axis: Axis,
}

/// What is carrying the item: a pointer, with where its stroke was first reported and where it is now, or the keyboard.
#[derive(Clone, Copy)]
enum Hand {
    Pointer { pressed: (f32, f32), at: (f32, f32) },
    Keys,
}

/// A stroke in progress; the rects are frozen at the press so the slots the pointer is measured against are the ones the user saw when they picked the item up, and so a gap the stroke opens cannot move the slot that opened it.
#[derive(Clone)]
struct Held {
    from: Slot,
    to: Option<Slot>,
    zones: Rc<Vec<FrozenZone>>,
    hand: Hand,
}

impl Held {
    fn zone(&self, zone: usize) -> Option<&FrozenZone> {
        self.zones.iter().find(|frozen| frozen.zone == zone)
    }

    fn item_rect(&self, slot: Slot) -> Option<Rect> {
        self.zone(slot.zone)?.items.get(slot.index).copied()
    }

    fn is_noop(&self, to: Slot) -> bool {
        to.zone == self.from.zone
            && (to.index == self.from.index || to.index == self.from.index + 1)
    }

    /// Every distinct drop target in reading order, with the two slots either side of the carried item folded into its own.
    fn targets(&self, reach: Reach) -> Vec<Slot> {
        let mut zones: Vec<&FrozenZone> = self
            .zones
            .iter()
            .filter(|frozen| reach == Reach::AnyZone || frozen.zone == self.from.zone)
            .collect();
        zones.sort_by_key(|frozen| frozen.zone);
        zones
            .into_iter()
            .flat_map(|frozen| {
                (0..=frozen.items.len()).map(move |index| Slot::new(frozen.zone, index))
            })
            .filter(|slot| !(slot.zone == self.from.zone && slot.index == self.from.index + 1))
            .collect()
    }

    fn stepped(&self, direction: isize, reach: Reach) -> Slot {
        let targets = self.targets(reach);
        let home = match self.to {
            Some(to) if !self.is_noop(to) => to,
            _ => self.from,
        };
        targets
            .iter()
            .position(|slot| *slot == home)
            .and_then(|at| at.checked_add_signed(direction))
            .and_then(|next| targets.get(next).copied())
            .unwrap_or(home)
    }
}

struct Zone {
    rect: RwSignal<Rect>,
    items: Rc<RefCell<Vec<Option<RwSignal<Rect>>>>>,
    count: Reactive<usize>,
    axis: Axis,
    gap: f32,
}

impl Zone {
    fn freeze(&self, zone: usize) -> FrozenZone {
        let len = self.count.get();
        let items = self
            .items
            .borrow()
            .iter()
            .take(len)
            .map(|slot| slot.as_ref().map(|rect| rect.peek()).unwrap_or_default())
            .collect();
        FrozenZone {
            zone,
            rect: self.rect.peek(),
            items,
            axis: self.axis,
        }
    }

    fn live_item(&self, index: usize) -> Option<Rect> {
        if index >= self.count.get() {
            return None;
        }
        self.items
            .borrow()
            .get(index)
            .copied()
            .flatten()
            .map(|rect| rect.get())
    }
}

type MoveHandler = Rc<dyn Fn(Slot, Slot)>;
type DetachHandler = Rc<dyn Fn(Slot, (f32, f32))>;

struct Group {
    zones: RefCell<Vec<(usize, Rc<Zone>)>>,
    held: RwSignal<Option<Held>>,
    transaction: Transaction<Option<Held>>,
    on_move: RefCell<Option<MoveHandler>>,
    on_detach: RefCell<Option<DetachHandler>>,
    threshold: Cell<f32>,
    drop_preview: Cell<DropPreview>,
    reach: Cell<Reach>,
}

/// A set of containers, built with [`zone`](Self::zone), whose items can be dragged within and between them, or moved the same way with Space/Enter and the arrow keys; the group only reports moves through [`on_move`](Self::on_move) and [`on_detach`](Self::on_detach) and writes nothing itself.
///
/// The options are read as each container is built, so they are set before the first [`zone`](Self::zone).
#[derive(Clone)]
pub struct ReorderGroup(Rc<Group>);

impl Default for ReorderGroup {
    fn default() -> Self {
        Self::new()
    }
}

impl ReorderGroup {
    pub fn new() -> Self {
        let held = signal(None);
        Self(Rc::new(Group {
            zones: RefCell::new(Vec::new()),
            held,
            transaction: Transaction::new(held),
            on_move: RefCell::new(None),
            on_detach: RefCell::new(None),
            threshold: Cell::new(4.0),
            drop_preview: Cell::new(DropPreview::default()),
            reach: Cell::new(Reach::default()),
        }))
    }

    pub fn on_move(self, f: impl Fn(Slot, Slot) + 'static) -> Self {
        *self.0.on_move.borrow_mut() = Some(Rc::new(f));
        self
    }

    /// Lets an item dropped outside every container leave the group, told where it was dropped in surface coordinates.
    pub fn on_detach(self, f: impl Fn(Slot, (f32, f32)) + 'static) -> Self {
        *self.0.on_detach.borrow_mut() = Some(Rc::new(f));
        self
    }

    /// How far the pointer travels before a press on an item is a drag rather than a click on it.
    pub fn drag_threshold(self, px: f32) -> Self {
        self.0.threshold.set(px.max(0.0));
        self
    }

    pub fn drop_preview(self, preview: DropPreview) -> Self {
        self.0.drop_preview.set(preview);
        self
    }

    pub fn reach(self, reach: Reach) -> Self {
        self.0.reach.set(reach);
        self
    }

    /// Where the item in hand would land, reactively: `None` with nothing in hand or when it would detach.
    pub fn target(&self) -> Option<Slot> {
        self.0.held.get().and_then(|held| held.to)
    }

    /// Which item is in hand, reactively.
    pub fn carried(&self) -> Option<Slot> {
        self.0.held.get().map(|held| held.from)
    }

    /// One container of the group, holding `count` items built by `item`.
    pub fn zone(&self, props: ReorderZoneProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
        let ReorderZoneProps {
            zone: index,
            count,
            item,
            row,
            gap,
            min_extent,
        } = props;
        let axis = if row {
            Axis::Horizontal
        } else {
            Axis::Vertical
        };

        let source_count = count.clone();
        let source = move || (0..source_count.get()).collect::<Vec<usize>>();
        let zone_items: Rc<RefCell<Vec<Option<RwSignal<Rect>>>>> =
            Rc::new(RefCell::new(Vec::new()));
        let build = {
            let group = self.clone();
            let zone_items = zone_items.clone();
            let count = count.clone();
            move |at: usize| {
                group.item(
                    Slot::new(index, at),
                    ItemLayout {
                        axis,
                        gap,
                        count: count.clone(),
                    },
                    &item,
                    &zone_items,
                )
            }
        };
        let list = ReactiveList::new(source, |at: &usize| *at, build, gap)?;
        let list = if row { list.as_row() } else { list };

        let mut children = vec![box_item(list)];
        if self.0.drop_preview.get() == DropPreview::Line {
            children.push(insertion_line(self.0.held, index, axis, gap)?);
        }
        let container = Container::new(
            LayoutStyle::column()
                .min_width(min_extent)
                .min_height(min_extent),
            children,
        )?
        .role(Role::List);
        let rect = track_layout(container.layout_node()).ok_or_else(|| {
            LayoutError::Engine("reorder zone: its layout node is not in the live tree".into())
        })?;
        self.register(
            index,
            Zone {
                rect,
                items: zone_items,
                count,
                axis,
                gap,
            },
        );
        Ok(box_item(container))
    }

    fn register(&self, index: usize, zone: Zone) {
        let zone = Rc::new(zone);
        self.0.zones.borrow_mut().push((index, Rc::clone(&zone)));
        let group = Rc::downgrade(&self.0);
        on_cleanup(move || {
            if let Some(group) = group.upgrade() {
                group
                    .zones
                    .borrow_mut()
                    .retain(|(_, registered)| !Rc::ptr_eq(registered, &zone));
            }
        });
    }

    fn item(
        &self,
        slot: Slot,
        layout: ItemLayout,
        item: &ItemBuilder,
        zone_items: &Rc<RefCell<Vec<Option<RwSignal<Rect>>>>>,
    ) -> Result<Box<dyn LayoutItem>, LayoutError> {
        let child = item(slot.index)?;
        let rect = track_layout(child.layout_node()).ok_or_else(|| {
            LayoutError::Engine(
                "reorder zone: an item's layout node is not in the live tree".into(),
            )
        })?;
        remember(zone_items, slot.index, rect);

        let ItemLayout { axis, gap, count } = layout;
        let held = self.0.held;
        let preview = self.0.drop_preview.get();
        let reach = self.0.reach.get();
        let mut wrapper =
            StyledContainer::new(LayoutStyle::new(), |_| RectStyle::default(), vec![child])?;
        if preview == DropPreview::Gap {
            // Read only inside the restyle effect: `item` runs inside the list's reconcile effect, and a read here would reconcile the list from inside its own event dispatch.
            wrapper = wrapper
                .styled_by(move || gap_style(slot, count.get(), held.get().as_ref(), axis, gap));
        }
        if reach == Reach::OwnZone {
            let group = self.0.clone();
            wrapper = wrapper
                .drag_axis(match axis {
                    Axis::Horizontal => DragAxis::Horizontal,
                    Axis::Vertical => DragAxis::Vertical,
                })
                .drag_within(move || group.bounds(slot, rect));
        }
        let wrapper = wrapper
            .with_transform({
                let group = self.0.clone();
                move |laid| {
                    let held = held.get().filter(|held| held.from == slot)?;
                    let (x, y) = match (held.hand, preview) {
                        (Hand::Pointer { pressed, at }, _) => {
                            let was = held.item_rect(slot).unwrap_or(laid);
                            (at.0 - pressed.0 + was.x, at.1 - pressed.1 + was.y)
                        }
                        (Hand::Keys, DropPreview::Gap) => group.gap_origin(&held, held.to?)?,
                        (Hand::Keys, DropPreview::Line) => return None,
                    };
                    Some([1.0, 0.0, 0.0, 1.0, x - laid.x, y - laid.y])
                }
            })
            .control(Role::ListItem)
            .consumes_keys(|| ConsumedKeys::ACTIVATION | ConsumedKeys::ARROWS)
            .on_focus({
                let group = self.0.clone();
                move |now| {
                    if !now && group.held.peek().is_some_and(|held| held.from == slot) {
                        let _ = group.transaction.revert();
                    }
                }
            })
            .cursor(Reactive::of(move || {
                match held.get().is_some_and(|held| held.from == slot) {
                    true => Cursor::Grabbing,
                    false => Cursor::Grab,
                }
            }))
            .drag_threshold(self.0.threshold.get())
            .drag_transaction(self.0.transaction)
            .on_drag({
                let group = self.0.clone();
                move |x, y| {
                    let origin = rect.peek();
                    group.carry(slot, (origin.x + x, origin.y + y));
                }
            })
            .on_drag_end({
                let group = self.0.clone();
                move |_, _| group.land()
            })
            .on_focused_key({
                let group = self.0.clone();
                move |key: &Key| -> bool { group.key(slot, key) }
            });
        Ok(box_item(wrapper))
    }
}

/// What an item needs to know of the container it sits in.
struct ItemLayout {
    axis: Axis,
    gap: f32,
    count: Reactive<usize>,
}

impl Group {
    fn freeze(&self) -> Rc<Vec<FrozenZone>> {
        Rc::new(
            self.zones
                .borrow()
                .iter()
                .map(|(index, zone)| zone.freeze(*index))
                .collect(),
        )
    }

    fn live_zone(&self, zone: usize) -> Option<Rc<Zone>> {
        self.zones
            .borrow()
            .iter()
            .find(|(index, _)| *index == zone)
            .map(|(_, live)| Rc::clone(live))
    }

    fn hold(&self, next: Option<Held>) {
        let written = next.clone();
        if self
            .transaction
            .preview(move |held| *held = written)
            .is_err()
        {
            self.held.set(next);
        }
    }

    fn carry(&self, from: Slot, point: (f32, f32)) {
        let (zones, pressed) = match self.held.peek() {
            Some(Held {
                from: held_from,
                zones,
                hand: Hand::Pointer { pressed, .. },
                ..
            }) if held_from == from => (zones, pressed),
            _ => (self.freeze(), point),
        };
        let landing = match self.reach.get() {
            Reach::AnyZone => zones
                .iter()
                .find(|frozen| frozen.rect.contains(point.0, point.1)),
            Reach::OwnZone => zones.iter().find(|frozen| frozen.zone == from.zone),
        };
        let to = landing.map(|frozen| {
            Slot::new(
                frozen.zone,
                insertion_index(&frozen.items, point, frozen.axis),
            )
        });
        self.hold(Some(Held {
            from,
            to,
            zones,
            hand: Hand::Pointer { pressed, at: point },
        }));
    }

    /// Ends the stroke where it is and reports what it did, once.
    fn land(&self) {
        let Some(held) = self.held.peek() else {
            return;
        };
        self.hold(None);
        match (held.to, held.hand) {
            (Some(to), _) if !held.is_noop(to) => {
                let on_move = self.on_move.borrow().clone();
                if let Some(on_move) = on_move {
                    on_move(held.from, to);
                }
            }
            (None, Hand::Pointer { at, .. }) => {
                let on_detach = self.on_detach.borrow().clone();
                if let Some(on_detach) = on_detach {
                    on_detach(held.from, at);
                }
            }
            _ => {}
        }
    }

    fn key(&self, slot: Slot, key: &Key) -> bool {
        let held = self.held.peek();
        let mine = held.clone().filter(|held| held.from == slot);
        let Key::Named(named) = key else {
            return false;
        };
        let reach = self.reach.get();
        match (mine, named) {
            (None, NamedKey::Space | NamedKey::Enter) => {
                if held.is_some() || self.transaction.begin().is_err() {
                    return false;
                }
                self.hold(Some(Held {
                    from: slot,
                    to: Some(slot),
                    zones: self.freeze(),
                    hand: Hand::Keys,
                }));
                true
            }
            (Some(held), NamedKey::ArrowLeft | NamedKey::ArrowUp) => {
                self.hold(Some(Held {
                    to: Some(held.stepped(-1, reach)),
                    ..held
                }));
                true
            }
            (Some(held), NamedKey::ArrowRight | NamedKey::ArrowDown) => {
                self.hold(Some(Held {
                    to: Some(held.stepped(1, reach)),
                    ..held
                }));
                true
            }
            (Some(_), NamedKey::Space | NamedKey::Enter) => {
                self.land();
                let _ = self.transaction.commit();
                true
            }
            (Some(_), NamedKey::Escape) => {
                let _ = self.transaction.revert();
                true
            }
            _ => false,
        }
    }

    /// The box an item of `slot` may be carried in under [`Reach::OwnZone`], in that item's own coordinates (its laid-out rect is `mine`).
    ///
    /// Frozen once a stroke is running: a gap moves the items and grows the live box, which would move the clamped point, which moves the gap, and at the edge the item would swap with its neighbour for ever on a pointer standing still.
    fn bounds(&self, slot: Slot, mine: RwSignal<Rect>) -> Rect {
        let zone = match self.held.peek() {
            Some(held) => held.zone(slot.zone).map(|frozen| frozen.rect),
            None => self.live_zone(slot.zone).map(|live| live.rect.peek()),
        }
        .unwrap_or_default();
        let me = mine.peek();
        Rect::new(zone.x - me.x, zone.y - me.y, zone.width, zone.height)
    }

    /// Where the carried item's box starts when it is drawn in the gap opened at `to`, read off the live layout the gap is part of.
    fn gap_origin(&self, held: &Held, to: Slot) -> Option<(f32, f32)> {
        let zone = self.live_zone(to.zone)?;
        let carried = held.item_rect(held.from)?;
        let opening = zone.axis.of((carried.width, carried.height)) + zone.gap;
        let along = |rect: Rect, by: f32| match zone.axis {
            Axis::Horizontal => (rect.x + by, rect.y),
            Axis::Vertical => (rect.x, rect.y + by),
        };
        if let Some(after) = zone.live_item(to.index) {
            return Some(along(after, -opening));
        }
        if let Some(before) = to.index.checked_sub(1).and_then(|at| zone.live_item(at)) {
            return Some(along(
                before,
                zone.axis.of((before.width, before.height)) + zone.gap,
            ));
        }
        let empty = zone.rect.get();
        Some((empty.x, empty.y))
    }
}

/// The rule [`DropPreview::Line`] draws across the container at the slot the drop would land in.
fn insertion_line(
    held: RwSignal<Option<Held>>,
    zone: usize,
    axis: Axis,
    gap: f32,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let line_box = move || {
        let at_origin = LayoutStyle::new()
            .absolute()
            .inset_top(0.0)
            .inset_start(0.0);
        let (across, along) = (
            SizeDimension::Px(shared::focus_ring_width()),
            SizeDimension::Percent(1.0),
        );
        match axis {
            Axis::Horizontal => at_origin.width(across).height(along),
            Axis::Vertical => at_origin.width(along).height(across),
        }
    };
    let line = StyledContainer::new(
        line_box(),
        move |_| match held.get().and_then(|held| held.to) {
            Some(to) if to.zone == zone => RectStyle::default().with_fill(shared::accent()),
            _ => RectStyle::default(),
        },
        vec![],
    )?
    .styled_by(line_box)
    .input_transparent()
    .with_transform(move |rect| {
        let held = held.get()?;
        let to = held.to.filter(|to| to.zone == zone)?;
        let along =
            line_position(held.zone(zone)?, to.index, gap) - shared::focus_ring_width() / 2.0;
        Some(match axis {
            Axis::Horizontal => [1.0, 0.0, 0.0, 1.0, along - rect.x, 0.0],
            Axis::Vertical => [1.0, 0.0, 0.0, 1.0, 0.0, along - rect.y],
        })
    });
    Ok(box_item(line))
}

/// The layout style [`DropPreview::Gap`] gives the item at `slot` of a container holding `len`: the slot the carried item came out of pulled shut, and one of its size opened where the drop would land.
///
/// The two are added, never chosen between: the carried item can also be the one the gap belongs to (the last of a container, dragged past the end), and choosing let one item open two holes. Both are the carried item's extent plus the container's spacing, so they cancel exactly and a container mid-drag within itself stays the size it was.
fn gap_style(slot: Slot, len: usize, held: Option<&Held>, axis: Axis, gap: f32) -> LayoutStyle {
    let base = LayoutStyle::new();
    let Some(held) = held else { return base };
    let opening = held
        .item_rect(held.from)
        .map_or(0.0, |rect| axis.of((rect.width, rect.height)))
        + gap;
    let opens_before = held.to == Some(slot);
    let opens_after = slot.index + 1 == len
        && held
            .to
            .is_some_and(|to| to.zone == slot.zone && to.index >= len);
    let closes = held.from == slot;
    let start = if opens_before { opening } else { 0.0 };
    let end = if opens_after { opening } else { 0.0 } - if closes { opening } else { 0.0 };
    match axis {
        Axis::Horizontal => base.margin_inline_start(start).margin_inline_end(end),
        Axis::Vertical => base.margin_block_start(start).margin_block_end(end),
    }
}

fn remember(items: &Rc<RefCell<Vec<Option<RwSignal<Rect>>>>>, index: usize, rect: RwSignal<Rect>) {
    let mut slots = items.borrow_mut();
    if slots.len() <= index {
        slots.resize(index + 1, None);
    }
    slots[index] = Some(rect);
}

fn line_position(zone: &FrozenZone, index: usize, gap: f32) -> f32 {
    let start = |rect: &Rect| zone.axis.of((rect.x, rect.y));
    let end = |rect: &Rect| match zone.axis {
        Axis::Horizontal => rect.x + rect.width,
        Axis::Vertical => rect.y + rect.height,
    };
    let items = &zone.items;
    match (
        index.checked_sub(1).and_then(|before| items.get(before)),
        items.get(index),
    ) {
        (None, None) => start(&zone.rect),
        (None, Some(first)) => start(first) - gap / 2.0,
        (Some(last), None) => end(last) + gap / 2.0,
        (Some(before), Some(after)) => (end(before) + start(after)) / 2.0,
    }
}

/// One container of a [`ReorderGroup`].
#[derive(Props)]
pub struct ReorderZoneProps {
    /// This container's number in the group. Keyboard traversal walks containers in this order.
    #[props(default)]
    pub zone: usize,
    #[props(into, default)]
    pub count: Reactive<usize>,
    #[props(default = Rc::new(|_| Err(LayoutError::Engine("reorder zone: no item builder".into()))))]
    pub item: ItemBuilder,
    #[props(default)]
    pub row: bool,
    #[props(default)]
    pub gap: f32,
    /// The smallest the container gets on both axes, so an empty one is still somewhere to drop.
    #[props(default = shared::MIN_TARGET)]
    pub min_extent: f32,
}

#[cfg(test)]
#[path = "reorder_zones.test.rs"]
mod tests;
