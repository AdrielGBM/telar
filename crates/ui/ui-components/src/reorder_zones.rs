//! [`ReorderGroup`]: items dragged within and between several containers, with an insertion line, drag-out-to-detach and a keyboard path.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use telar_macros::Props;

use geometry_core::Rect;
use layout_core::{LayoutError, LayoutStyle, SizeDimension};
use platform_core::{Cursor, Key, NamedKey};
use reactive_core::{Reactive, RwSignal, Transaction, on_cleanup, signal};
use renderer_core::{RectStyle, ShapeStyle};
use ui_core::focus::Role;
use ui_core::{
    Axis, LayoutItem, ReactiveList, StyledContainer, box_item, insertion_index, track_layout,
};

use crate::reorderable::ItemBuilder;
use crate::shared;

/// Thickness of the insertion line.
const LINE: f32 = 2.0;

/// An item's place: which container, and where in it. As a drop target, `index` counts slots in that container before the move, the frame [`ui_core::apply_move`] reads.
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

/// Moves the item at `from` into the slot `to` across `zones`, the storage-side twin of a [`ReorderGroup`] move. Returns whether anything moved.
pub fn apply_zone_move<T>(zones: &mut [Vec<T>], from: Slot, to: Slot) -> bool {
    if from.zone == to.zone {
        return zones
            .get_mut(from.zone)
            .is_some_and(|items| ui_core::apply_move(items, from.index, to.index));
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

/// A stroke in progress, pointer or keyboard; the rects are frozen at the press so the slots the pointer is measured against are the ones the user saw when they picked the item up.
#[derive(Clone)]
struct Held {
    from: Slot,
    to: Option<Slot>,
    zones: Rc<Vec<FrozenZone>>,
    from_point: (f32, f32),
    at: (f32, f32),
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
    fn targets(&self) -> Vec<Slot> {
        let mut zones: Vec<&FrozenZone> = self.zones.iter().collect();
        zones.sort_by_key(|frozen| frozen.zone);
        zones
            .into_iter()
            .flat_map(|frozen| {
                (0..=frozen.items.len()).map(move |index| Slot::new(frozen.zone, index))
            })
            .filter(|slot| !(slot.zone == self.from.zone && slot.index == self.from.index + 1))
            .collect()
    }

    fn stepped(&self, direction: isize) -> Option<Slot> {
        let targets = self.targets();
        let home = match self.to {
            Some(to) if self.is_noop(to) => self.from,
            Some(to) => to,
            None => self.from,
        };
        let at = targets.iter().position(|slot| *slot == home)?;
        let next = at.checked_add_signed(direction)?;
        targets.get(next).copied().or(Some(home))
    }
}

struct Zone {
    rect: RwSignal<Rect>,
    items: Rc<RefCell<Vec<Option<RwSignal<Rect>>>>>,
    count: Reactive<usize>,
    axis: Axis,
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
}

/// A set of containers, built with [`zone`](Self::zone), whose items can be dragged within and between them, or moved the same way with Space/Enter and the arrow keys; the group only reports moves through [`on_move`](Self::on_move) and [`on_detach`](Self::on_detach) and writes nothing itself.
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
            move |at: usize| group.item(Slot::new(index, at), &item, &zone_items)
        };
        let list = ReactiveList::new(source, |at: &usize| *at, build, gap)?;
        let list = if row { list.as_row() } else { list };

        let line_held = self.0.held;
        let line_paint_held = self.0.held;
        let line = StyledContainer::new(
            match axis {
                Axis::Horizontal => LayoutStyle::new()
                    .absolute()
                    .inset_top(0.0)
                    .inset_start(0.0)
                    .width(LINE)
                    .height(SizeDimension::Percent(1.0)),
                Axis::Vertical => LayoutStyle::new()
                    .absolute()
                    .inset_top(0.0)
                    .inset_start(0.0)
                    .width(SizeDimension::Percent(1.0))
                    .height(LINE),
            },
            move |_| match line_paint_held.get().and_then(|held| held.to) {
                Some(to) if to.zone == index => RectStyle::default().with_fill(shared::accent()),
                _ => RectStyle::default(),
            },
            vec![],
        )?
        .input_transparent()
        .with_transform(move |rect| {
            let held = line_held.get()?;
            let to = held.to.filter(|to| to.zone == index)?;
            let along = line_position(held.zone(index)?, to.index, gap) - LINE / 2.0;
            Some(match axis {
                Axis::Horizontal => [1.0, 0.0, 0.0, 1.0, along - rect.x, 0.0],
                Axis::Vertical => [1.0, 0.0, 0.0, 1.0, 0.0, along - rect.y],
            })
        });

        let container = StyledContainer::new(
            LayoutStyle::new()
                .flex_column()
                .min_width(min_extent)
                .min_height(min_extent),
            |_| RectStyle::default(),
            vec![box_item(list), box_item(line)],
        )?;
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

        let held = self.0.held;
        let wrapper = StyledContainer::new(
            LayoutStyle::new(),
            |_| RectStyle::default(),
            vec![box_item(child)],
        )?
        .with_transform(move |laid| {
            let held = held.get().filter(|held| held.from == slot)?;
            let was = held.item_rect(slot).unwrap_or(laid);
            Some([
                1.0,
                0.0,
                0.0,
                1.0,
                held.at.0 - held.from_point.0 + was.x - laid.x,
                held.at.1 - held.from_point.1 + was.y - laid.y,
            ])
        })
        .control(Role::ListItem)
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

    fn preview(&self, next: Option<Held>) {
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
        let (zones, from_point) = match self.held.peek() {
            Some(held) if held.from == from => (held.zones, held.from_point),
            _ => (self.freeze(), point),
        };
        let to = zones
            .iter()
            .find(|frozen| frozen.rect.contains(point.0, point.1))
            .map(|frozen| {
                Slot::new(
                    frozen.zone,
                    insertion_index(&frozen.items, point, frozen.axis),
                )
            });
        self.preview(Some(Held {
            from,
            to,
            zones,
            from_point,
            at: point,
        }));
    }

    /// Ends the stroke where it is and reports what it did, once.
    fn land(&self) {
        let Some(held) = self.held.peek() else {
            return;
        };
        self.preview(None);
        match held.to {
            Some(to) if !held.is_noop(to) => {
                let on_move = self.on_move.borrow().clone();
                if let Some(on_move) = on_move {
                    on_move(held.from, to);
                }
            }
            Some(_) => {}
            None => {
                let on_detach = self.on_detach.borrow().clone();
                if let Some(on_detach) = on_detach {
                    on_detach(held.from, held.at);
                }
            }
        }
    }

    fn key(&self, slot: Slot, key: &Key) -> bool {
        let held = self.held.peek();
        let mine = held.clone().filter(|held| held.from == slot);
        let Key::Named(named) = key else {
            return false;
        };
        match (mine, named) {
            (None, NamedKey::Space | NamedKey::Enter) => {
                if held.is_some() || self.transaction.begin().is_err() {
                    return false;
                }
                self.preview(Some(Held {
                    from: slot,
                    to: Some(slot),
                    zones: self.freeze(),
                    from_point: (0.0, 0.0),
                    at: (0.0, 0.0),
                }));
                true
            }
            (Some(held), NamedKey::ArrowLeft | NamedKey::ArrowUp) => {
                self.preview(Some(Held {
                    to: held.stepped(-1),
                    ..held
                }));
                true
            }
            (Some(held), NamedKey::ArrowRight | NamedKey::ArrowDown) => {
                self.preview(Some(Held {
                    to: held.stepped(1),
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
    #[props(default = 24.0)]
    pub min_extent: f32,
}

#[cfg(test)]
#[path = "reorder_zones_test.rs"]
mod tests;
