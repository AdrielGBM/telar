# Drag session

A **drag session** carries a value of the app's own type with the pointer, from a box in one widget to a place
in another, inside the app: a row of a list dropped between two others, onto a folder of a tree in a side
panel, or onto the header of a collapsed section. It is not the operating system's drag and drop
(`drag_source` and `drop_target` on a box), which carries data into the app or out of it.

Every box that takes part shares one `DragSession<P>`, whichever widget built it:

- an **item** is a box the pointer picks a payload up from;
- an **area** is a box a payload can be dropped on, between its items or onto one of them, as a function of
  the app's accepts.

```rust
use telar::{AreaLayout, Axis, DragItem, DragSession, DropArea, DropPlace};

#[derive(Clone)]
enum Carried {
    Text(TextId),
    Segment(SegmentId),
}

let session = DragSession::<Carried>::new();

// An item: a press that travels past its threshold picks the payload up.
let row = session.item(
    row,
    DragItem::new(move || Some(Carried::Text(id)))
        .label(move || title.get())
        .on_detach(|payload, at| { /* let go over no area that accepts it */ }),
);

// An area: a box whose items run along an axis, with the app's accept and drop.
let section_node = header.layout_node();
let laid = telar::track_layout(section_node).unwrap();
session.area(
    DropArea::new(
        section_node,
        Axis::Vertical,
        |payload, place| matches!((payload, place), (Carried::Segment(_), DropPlace::Inside(0))),
        move |payload, place| { /* the drop landed */ },
    )
    .layout(move || {
        let at = laid.peek();
        AreaLayout { bounds: at, items: vec![at] }
    })
    .on_hold(move |_, _| expand()),
);
```

`ReorderGroup` in `telar-components` is built on a session: its containers are areas and its items are items,
so a list made with it takes payloads from any other widget of the same session and gives its own to them.

## Items

`session.item(container, DragItem::new(payload))` makes a `StyledContainer` an item. It sets the box's drag
handlers, threshold and transaction, so none of them is set on it elsewhere, and shows the grab cursor over it
and the grabbing one while it is the item carried.

| `DragItem` | Says |
| --- | --- |
| `new(payload)` | What the item carries, read as the drag starts. `None` leaves the stroke undragged. |
| `label(f)` | What the payload is called, read as the drag starts, for whoever draws the label beside the pointer. |
| `threshold(px)` | How far the pointer travels before a press is a drag rather than a click. 4 px unless set. |
| `on_detach(f)` | Told the payload and where on the surface it was let go, when it is let go over no area that accepts it. |

## Areas

`session.area(DropArea::new(node, axis, accepts, on_drop))` registers an area until the owner that registered it
is disposed. `node` is the area's box; `layout` says where the area and its items are, **in the space the
node's children are laid out in**. Inside a scroll area that is the content's own coordinates, which do not
change as it scrolls, so an area measured once stays right while the content scrolls under the pointer: the
session maps the pointer into each area's space through the scroll offsets as they are at that moment. Without
`layout` an area is its box's own rect with no items.

A drop lands at a `DropPlace`: `Between(n)`, the gap before the `n`th item (`Between(len)` is after the last),
or `Inside(n)`, onto the `n`th item. With the pointer at a point of an area:

- **Over an item that accepts the payload inside**, the middle half of the item is `Inside`, and each outer
  quarter the gap on that side.
- **Over an item that does not**, each half is the gap on its side.
- **Between items or past them**, it is the gap the pointer is in.
- A place the area refuses gives way to the other one the pointer is near: a refused gap next to an item that
  takes the payload is that item, and a refused inside is the gap. When both are refused the area takes
  nothing there.

Areas drawn inside other areas are asked first, and an area that takes nothing at the pointer lets the one
around it answer. The drop is told to the area with the place, in the area's own count; a widget that has its
own way of counting items maps it (`ReorderGroup` reports a `DropPlace<Slot>`).

`on_hold(f)` is told which item of the area the pointer has rested on, once it has rested there for the
session's hold delay (700 ms unless `DragSession::hold_delay` says otherwise): once per stay, whatever the area
accepts there, which is what opens a collapsed folder held over. Moving to another item starts the wait again.

## What a widget draws

The session draws nothing. Each widget reads it, reactively, and draws its own indicator:

| `DragSession` | Answers |
| --- | --- |
| `carried()` | The payload in hand. |
| `carried_from()` | The item it was picked up from. |
| `over()`, `place_in(area)` | The area and place the payload would land at if it were let go now. |
| `label()`, `pointer()` | What the payload is called, and where the pointer is on the surface. |
| `travel()` | How far the payload has been carried, in the space its item is laid out in, counting the content scrolled under a still pointer: what moves the item along with the pointer. |

`ReorderGroup` draws a line at a gap, highlights an item a payload would land onto, and with `label(f)` shows
the label beside the pointer, in an overlay that takes no pointer (`Overlay::click_through`), so it stays
readable once the payload has left the scroll area that clips its item.

## Scrolling near an edge

While a payload is carried, a scroll area that holds one of the session's areas scrolls when the pointer is
within 40 px of an edge it can still scroll towards (or a quarter of the area, when that is less), faster the
nearer the pointer is to the edge, up to 1200 px a second. The innermost such scroll area scrolls first. It
runs on the timer clock rather than on animation frames, so it is not shortened by the reduced-motion
preference, and it stops at the end of the content, when the pointer leaves the edge and when the stroke ends.
After each step the pointer is measured again against what scrolled under it.

## Ending a drag

Letting go drops the payload on the area under the pointer, or tells the item it was let go over none. Escape,
another button pressed mid-stroke or the window losing focus cancels it: the drag's transaction puts the
session back as it was before the press, and the scroll and the hold stop.

## The keyboard

A session is the pointer's. A widget moves its items with keys of its own and reports the move the way a drop
is reported, so the app has one place to act on either. `ReorderGroup` steps the focused item one gap earlier or
later with Alt+↑ and Alt+↓ and takes no other key: Enter, Space and the bare arrows stay the app's, so a row
can open on Enter.

## Targets

The same on every target: the session works from the pointer events every target delivers and from the layout
every target shares.

| Target | Notes |
| --- | --- |
| web-dom | A scroll area whose content the document scrolls is asked for its offset rather than set, so the areas are measured against the scrolled content once the document reports where it put it. |
| Android, touch on any target | A press on an item inside a scroll area is taken by the item once it travels past the threshold, as on a desktop: the finger drags the item, not the list. |
| TUI | The same, on the cell grid. |
| headless | The same. `telar::testing::route` with presses and moves drives a stroke, and `advance_time` lets the hold delay and the scroll steps pass; a play's `drag` carries an item in one step. |
