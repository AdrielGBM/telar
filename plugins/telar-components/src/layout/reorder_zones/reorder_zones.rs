//! [`ReorderGroup`]: items dragged within and between several containers of a [`DragSession`], dropped between items or onto one, with a drop preview, drag-out-to-detach and a keyboard step.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use telar::{
    AreaLayout, Axis, Border, BorderRadius, Chord, Container, DragAxis, DragItem, DragSession,
    DropArea, DropAreaId, DropPlace, Key, LayoutError, LayoutItem, LayoutStyle, NamedKey, NodeId,
    Props, Reactive, ReactiveList, Rect, RectStyle, RwSignal, ShapeStyle, SizeDimension,
    StyledContainer, Transform, after_settle, box_item, chord_pressed,
    focus::{self, FocusId, Role},
    on_cleanup, track_layout,
};

use crate::shared::{self, drag_label::drag_label};

/// One item's widget, built on demand from its position in the caller's storage.
pub type ItemBuilder = Rc<dyn Fn(usize) -> Result<Box<dyn LayoutItem>, LayoutError>>;

/// An item's place: which container, and where in it. As a drop place, `index` counts slots in that container before the move, the frame [`telar::apply_move`] reads.
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

/// How a [`ReorderGroup`] shows the gap one of its own items would land in. A drop onto an item highlights that item, and a payload from another widget is always shown with a line.
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
    /// Anywhere the pointer goes: it lands in whichever container or other area of the session is under it, and outside them all it detaches.
    #[default]
    AnyZone,
    /// Along its own container's axis and inside it, landing only there: a single list reordered in place.
    OwnZone,
}

/// Moves the item at `from` into the slot `to` across `zones`, the storage-side twin of a [`ReorderGroup`] drop between two items. Returns whether anything moved.
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

/// A container's layout and items, in the coordinates of the content it sits in.
#[derive(Clone)]
struct FrozenZone {
    zone: usize,
    rect: Rect,
    items: Vec<Rect>,
    axis: Axis,
}

struct Zone {
    area: Cell<Option<DropAreaId>>,
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

type Accepts<P> = Rc<dyn Fn(&P, DropPlace<Slot>) -> bool>;
type OnDrop<P> = Rc<dyn Fn(P, DropPlace<Slot>)>;
type OnDetach<P> = Rc<dyn Fn(P, (f32, f32))>;
type Label = Rc<dyn Fn(Slot) -> String>;

/// What a group asks and tells its caller.
struct Reports<P> {
    accepts: Option<Accepts<P>>,
    on_drop: Option<OnDrop<P>>,
    on_detach: Option<OnDetach<P>>,
    label: Option<Label>,
}

impl<P> Default for Reports<P> {
    fn default() -> Self {
        Self {
            accepts: None,
            on_drop: None,
            on_detach: None,
            label: None,
        }
    }
}

struct Group<P: Clone + 'static> {
    session: DragSession<P>,
    payload: Rc<dyn Fn(Slot) -> P>,
    zones: RefCell<Vec<(usize, Rc<Zone>)>>,
    /// Each item's box with the slot it stands for: how the group tells which of its items the session carries.
    items: RefCell<Vec<(NodeId, Slot, Option<FocusId>)>>,
    /// The containers as they were when an item of the group was picked up, so the slots the pointer is measured against are the ones the user saw, and a gap the stroke opens cannot move the slot that opened it.
    frozen: RefCell<Rc<Vec<FrozenZone>>>,
    reports: RefCell<Reports<P>>,
    threshold: Cell<f32>,
    drop_preview: Cell<DropPreview>,
    reach: Cell<Reach>,
}

/// A set of containers, built with [`zone`](Self::zone), whose items are dragged within and between them, and to and from the other areas of the same [`DragSession`].
///
/// An item carries what `payload` makes of its [`Slot`]. A container takes a payload between two of its items, or onto one where [`accepts`](Self::accepts) allows it, and the group reports the drop through [`on_drop`](Self::on_drop) and writes nothing itself. Alt+↑ and Alt+↓ on a focused item move it one gap earlier or later, reported the same way; the group takes no other key, so Enter and Space stay the caller's.
///
/// The options are read as each container is built, so they are set before the first [`zone`](Self::zone).
pub struct ReorderGroup<P: Clone + 'static>(Rc<Group<P>>);

impl<P: Clone + 'static> Clone for ReorderGroup<P> {
    fn clone(&self) -> Self {
        Self(Rc::clone(&self.0))
    }
}

impl<P: Clone + 'static> ReorderGroup<P> {
    /// A group whose items are carried in `session`, each as what `payload` makes of its slot.
    pub fn new(session: &DragSession<P>, payload: impl Fn(Slot) -> P + 'static) -> Self {
        Self(Rc::new(Group {
            session: session.clone(),
            payload: Rc::new(payload),
            zones: RefCell::new(Vec::new()),
            items: RefCell::new(Vec::new()),
            frozen: RefCell::new(Rc::new(Vec::new())),
            reports: RefCell::default(),
            threshold: Cell::new(4.0),
            drop_preview: Cell::new(DropPreview::default()),
            reach: Cell::new(Reach::default()),
        }))
    }

    /// Whether a payload may land at a place in the group. Without it every gap takes any payload and no item takes one onto it. Either way an item is never dropped onto itself, and under [`Reach::OwnZone`] nothing lands outside the container it came from.
    pub fn accepts(self, accepts: impl Fn(&P, DropPlace<Slot>) -> bool + 'static) -> Self {
        self.0.reports.borrow_mut().accepts = Some(Rc::new(accepts));
        self
    }

    /// Told what landed where. An item put back in the gap it came out of is not reported.
    pub fn on_drop(self, on_drop: impl Fn(P, DropPlace<Slot>) + 'static) -> Self {
        self.0.reports.borrow_mut().on_drop = Some(Rc::new(on_drop));
        self
    }

    /// Lets an item dropped over no area that accepts it leave the group, told where it was dropped in surface coordinates.
    pub fn on_detach(self, on_detach: impl Fn(P, (f32, f32)) + 'static) -> Self {
        self.0.reports.borrow_mut().on_detach = Some(Rc::new(on_detach));
        self
    }

    /// What an item is called, shown beside the pointer while it is carried: what keeps a payload readable once it has left the container that clips its item.
    pub fn label(self, label: impl Fn(Slot) -> String + 'static) -> Self {
        self.0.reports.borrow_mut().label = Some(Rc::new(label));
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

    /// Where in the group the payload in hand would land, reactively: `None` with nothing in hand or while it would land anywhere else.
    pub fn target(&self) -> Option<DropPlace<Slot>> {
        self.0.target()
    }

    /// Which item of the group is in hand, reactively.
    pub fn carried(&self) -> Option<Slot> {
        self.0.carried()
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
            let group = Rc::clone(&self.0);
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

        let container = Container::new(
            LayoutStyle::column()
                .min_width(min_extent)
                .min_height(min_extent),
            vec![box_item(list), insertion_line(&self.0, index, axis, gap)?],
        )?
        .role(Role::List);
        let node = container.layout_node();
        let rect = track_layout(node).ok_or_else(|| {
            LayoutError::Engine("reorder zone: its layout node is not in the live tree".into())
        })?;
        let zone = Rc::new(Zone {
            area: Cell::new(None),
            rect,
            items: zone_items,
            count,
            axis,
        });
        self.0.register(index, Rc::clone(&zone));
        let area = self.0.session.area(self.0.area(node, index, axis));
        zone.area.set(Some(area));
        Ok(box_item(container))
    }
}

/// What an item needs to know of the container it sits in.
struct ItemLayout {
    axis: Axis,
    gap: f32,
    count: Reactive<usize>,
}

impl<P: Clone + 'static> Group<P> {
    fn register(self: &Rc<Self>, index: usize, zone: Rc<Zone>) {
        self.zones.borrow_mut().push((index, Rc::clone(&zone)));
        let group = Rc::downgrade(self);
        on_cleanup(move || {
            if let Some(group) = group.upgrade() {
                group
                    .zones
                    .borrow_mut()
                    .retain(|(_, registered)| !Rc::ptr_eq(registered, &zone));
            }
        });
    }

    /// Container `zone`, laid out in `node`, as an area of the session.
    fn area(self: &Rc<Self>, node: NodeId, zone: usize, axis: Axis) -> DropArea<P> {
        let accepts = {
            let group = Rc::clone(self);
            move |payload: &P, place: DropPlace<usize>| {
                group.admits(
                    group.carried(),
                    payload,
                    place.map(|index| Slot::new(zone, index)),
                )
            }
        };
        let on_drop = {
            let group = Rc::clone(self);
            move |payload: P, place: DropPlace<usize>| {
                group.report(
                    group.carried(),
                    payload,
                    place.map(|index| Slot::new(zone, index)),
                )
            }
        };
        let group = Rc::clone(self);
        DropArea::new(node, axis, accepts, on_drop).layout(move || {
            group
                .measured(zone)
                .map(|measured| AreaLayout {
                    bounds: measured.rect,
                    items: measured.items,
                })
                .unwrap_or_default()
        })
    }

    fn item(
        self: &Rc<Self>,
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
        let label = self.reports.borrow().label.clone();
        let mut children = vec![child];
        if label.is_some() {
            let group = Rc::clone(self);
            children.push(drag_label(&self.session, move || {
                group.carried() == Some(slot)
            })?);
        }
        let highlight = {
            let group = Rc::clone(self);
            move |_: Rect| match group.target() == Some(DropPlace::Inside(slot)) {
                true => RectStyle::default()
                    .with_fill(shared::held_wash())
                    .with_border(Border::uniform(
                        shared::accent(),
                        shared::border_width_strong(),
                    ))
                    .with_radius(BorderRadius::all(shared::radius_item())),
                false => RectStyle::default(),
            }
        };
        let mut wrapper = StyledContainer::new(LayoutStyle::new(), highlight, children)?;
        if self.drop_preview.get() == DropPreview::Gap {
            let group = Rc::clone(self);
            // Read only inside the restyle effect: `item` runs inside the list's reconcile effect, and a read here would reconcile the list from inside its own event dispatch.
            wrapper = wrapper.styled_by(move || group.gap_style(slot, count.get(), axis, gap));
        }
        if self.reach.get() == Reach::OwnZone {
            let group = Rc::clone(self);
            wrapper = wrapper
                .drag_axis(match axis {
                    Axis::Horizontal => DragAxis::Horizontal,
                    Axis::Vertical => DragAxis::Vertical,
                })
                .drag_within(move || group.bounds(slot, rect));
        }
        let wrapper = wrapper
            .with_transform({
                let group = Rc::clone(self);
                move |laid| {
                    if group.carried() != Some(slot) {
                        return None;
                    }
                    let (dx, dy) = group.session.travel()?;
                    let was = group.measured_item(slot).unwrap_or(laid);
                    Some(Transform::translate(
                        was.x + dx - laid.x,
                        was.y + dy - laid.y,
                    ))
                }
            })
            .control(Role::ListItem)
            .on_focused_key({
                let group = Rc::clone(self);
                move |key: &Key| -> bool { group.step_key(slot, key) }
            });
        let node = wrapper.layout_node();
        self.items
            .borrow_mut()
            .push((node, slot, wrapper.focus_id()));
        let group = Rc::downgrade(self);
        on_cleanup(move || {
            if let Some(group) = group.upgrade() {
                group.items.borrow_mut().retain(|(at, _, _)| *at != node);
            }
        });
        let mut picked = DragItem::new({
            let group = Rc::clone(self);
            move || {
                group.freeze();
                Some((group.payload)(slot))
            }
        })
        .threshold(self.threshold.get());
        if let Some(label) = label {
            picked = picked.label(move || label(slot));
        }
        if let Some(on_detach) = self.reports.borrow().on_detach.clone() {
            picked = picked.on_detach(move |payload, at| on_detach(payload, at));
        }
        Ok(box_item(self.session.item(wrapper, picked)))
    }

    fn freeze(&self) {
        let frozen = self
            .zones
            .borrow()
            .iter()
            .map(|(index, zone)| zone.freeze(*index))
            .collect();
        *self.frozen.borrow_mut() = Rc::new(frozen);
    }

    fn carried(&self) -> Option<Slot> {
        let from = self.session.carried_from()?;
        self.items
            .borrow()
            .iter()
            .find(|(node, _, _)| *node == from)
            .map(|(_, slot, _)| *slot)
    }

    /// Gives focus to the item that stands at `slot`, once the move that put something there has been built.
    fn focus_at(self: &Rc<Self>, slot: Slot) {
        let group = Rc::downgrade(self);
        after_settle(move || {
            let Some(group) = group.upgrade() else {
                return;
            };
            let id = group
                .items
                .borrow()
                .iter()
                .find(|(_, at, _)| *at == slot)
                .and_then(|(_, _, id)| *id);
            if let Some(id) = id {
                focus::request(id);
            }
        });
    }

    fn target(&self) -> Option<DropPlace<Slot>> {
        let (area, place) = self.session.over()?;
        let zone = self
            .zones
            .borrow()
            .iter()
            .find(|(_, zone)| zone.area.get() == Some(area))
            .map(|(index, _)| *index)?;
        Some(place.map(|index| Slot::new(zone, index)))
    }

    /// Container `zone` as the pointer is measured against it: as it was when the group's own item in hand was picked up, or as it is now.
    fn measured(&self, zone: usize) -> Option<FrozenZone> {
        if self.carried().is_some()
            && let Some(frozen) = self
                .frozen
                .borrow()
                .iter()
                .find(|frozen| frozen.zone == zone)
        {
            return Some(frozen.clone());
        }
        let zones = self.zones.borrow();
        let (_, live) = zones.iter().find(|(index, _)| *index == zone)?;
        Some(live.freeze(zone))
    }

    fn measured_item(&self, slot: Slot) -> Option<Rect> {
        self.measured(slot.zone)?.items.get(slot.index).copied()
    }

    /// Whether `payload`, carried from `from` when it is an item of the group, may land at `place`.
    fn admits(&self, from: Option<Slot>, payload: &P, place: DropPlace<Slot>) -> bool {
        if self.reach.get() == Reach::OwnZone
            && from.is_none_or(|from| from.zone != place.at().zone)
        {
            return false;
        }
        if from.is_some_and(|from| place == DropPlace::Inside(from)) {
            return false;
        }
        let accepts = self.reports.borrow().accepts.clone();
        match accepts {
            Some(accepts) => accepts(payload, place),
            None => matches!(place, DropPlace::Between(_)),
        }
    }

    /// Tells the caller `payload` landed at `place`, unless it is an item of the group put back in the gap it came out of.
    fn report(&self, from: Option<Slot>, payload: P, place: DropPlace<Slot>) {
        if let (Some(from), DropPlace::Between(to)) = (from, place)
            && to.zone == from.zone
            && (to.index == from.index || to.index == from.index + 1)
        {
            return;
        }
        let on_drop = self.reports.borrow().on_drop.clone();
        if let Some(on_drop) = on_drop {
            on_drop(payload, place);
        }
    }

    fn step_key(self: &Rc<Self>, slot: Slot, key: &Key) -> bool {
        let earlier = Chord::new(Key::Named(NamedKey::ArrowUp)).with_alt();
        let later = Chord::new(Key::Named(NamedKey::ArrowDown)).with_alt();
        let by = if chord_pressed(&earlier, key) {
            -1
        } else if chord_pressed(&later, key) {
            1
        } else {
            return false;
        };
        let payload = (self.payload)(slot);
        if let Some(to) = self.stepped(slot, by, &payload) {
            self.report(Some(slot), payload, DropPlace::Between(to));
            let landed = match to.zone == slot.zone && to.index > slot.index {
                true => Slot::new(to.zone, to.index - 1),
                false => to,
            };
            self.focus_at(landed);
        }
        true
    }

    /// The gap `by` gaps away from the one `from` stands in, in reading order across the containers it may reach, passing over the gaps it is refused and the one just after it, which is where it already is.
    fn stepped(&self, from: Slot, by: isize, payload: &P) -> Option<Slot> {
        let mut zones: Vec<(usize, usize)> = self
            .zones
            .borrow()
            .iter()
            .filter(|(index, _)| self.reach.get() == Reach::AnyZone || *index == from.zone)
            .map(|(index, zone)| (*index, zone.count.get()))
            .collect();
        zones.sort_by_key(|(index, _)| *index);
        let gaps: Vec<Slot> = zones
            .into_iter()
            .flat_map(|(zone, len)| (0..=len).map(move |index| Slot::new(zone, index)))
            .filter(|slot| *slot != Slot::new(from.zone, from.index + 1))
            .filter(|slot| {
                *slot == from || self.admits(Some(from), payload, DropPlace::Between(*slot))
            })
            .collect();
        let at = gaps.iter().position(|slot| *slot == from)?;
        gaps.get(at.checked_add_signed(by)?).copied()
    }

    /// The box an item of `slot` may be carried in under [`Reach::OwnZone`], in that item's own coordinates (its laid-out rect is `mine`).
    ///
    /// Read off the frozen container once a stroke is running: a gap moves the items and grows the live box, which would move the clamped point, which moves the gap, and at the edge the item would swap with its neighbour for ever on a pointer standing still.
    fn bounds(&self, slot: Slot, mine: RwSignal<Rect>) -> Rect {
        let zone = self
            .measured(slot.zone)
            .map(|zone| zone.rect)
            .unwrap_or_default();
        let me = mine.peek();
        Rect::new(zone.x - me.x, zone.y - me.y, zone.width, zone.height)
    }

    /// The layout style [`DropPreview::Gap`] gives the item at `slot` of a container holding `len`: the slot the carried item came out of pulled shut, and one of its size opened where the drop would land.
    ///
    /// The two are added, never chosen between: the carried item can also be the one the gap belongs to (the last of a container, dragged past the end), and choosing let one item open two holes. Both are the carried item's extent plus the container's spacing, so they cancel exactly and a container mid-drag within itself stays the size it was.
    fn gap_style(&self, slot: Slot, len: usize, axis: Axis, gap: f32) -> LayoutStyle {
        let base = LayoutStyle::new();
        let Some(from) = self.carried() else {
            return base;
        };
        let to = match self.target() {
            Some(DropPlace::Between(to)) => Some(to),
            _ => None,
        };
        let opening = self
            .measured_item(from)
            .map_or(0.0, |rect| axis.of((rect.width, rect.height)))
            + gap;
        let opens_before = to == Some(slot);
        let opens_after =
            slot.index + 1 == len && to.is_some_and(|to| to.zone == slot.zone && to.index >= len);
        let closes = from == slot;
        let start = if opens_before { opening } else { 0.0 };
        let end = if opens_after { opening } else { 0.0 } - if closes { opening } else { 0.0 };
        match axis {
            Axis::Horizontal => base.margin_inline_start(start).margin_inline_end(end),
            Axis::Vertical => base.margin_block_start(start).margin_block_end(end),
        }
    }
}

/// The rule drawn across container `zone` at the gap a payload would land in: for a payload from elsewhere, and for one of the group's own under [`DropPreview::Line`].
fn insertion_line<P: Clone + 'static>(
    group: &Rc<Group<P>>,
    zone: usize,
    axis: Axis,
    gap: f32,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let at: Rc<dyn Fn() -> Option<usize>> = {
        let group = Rc::clone(group);
        Rc::new(move || {
            if group.drop_preview.get() == DropPreview::Gap && group.carried().is_some() {
                return None;
            }
            match group.target()? {
                DropPlace::Between(to) if to.zone == zone => Some(to.index),
                _ => None,
            }
        })
    };
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
        {
            let at = Rc::clone(&at);
            move |_| match at() {
                Some(_) => RectStyle::default().with_fill(shared::accent()),
                None => RectStyle::default(),
            }
        },
        vec![],
    )?
    .styled_by(line_box)
    .input_transparent()
    .with_transform({
        let group = Rc::clone(group);
        move |rect| {
            let index = at()?;
            let along = line_position(&group.measured(zone)?, index, gap)
                - shared::focus_ring_width() / 2.0;
            Some(match axis {
                Axis::Horizontal => Transform::translate(along - rect.x, 0.0),
                Axis::Vertical => Transform::translate(0.0, along - rect.y),
            })
        }
    });
    Ok(box_item(line))
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
    /// This container's number in the group. The keyboard steps through containers in this order.
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
