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

`ReorderGroup` and `TreeDrag` in `telar-components` are built on a session: a group's containers and a tree's
rows are areas, and their items and rows are items, so a list or a tree made with them takes payloads from any
other widget of the same session and gives its own to them.

## Items

`session.item(container, DragItem::new(payload))` makes a `StyledContainer` an item. It sets the box's drag
handlers and threshold, so neither is set on it elsewhere, and shows the grab cursor over it and the grabbing one
while it is the item carried.

| `DragItem` | Says |
| --- | --- |
| `new(payload)` | What the item carries, read as the drag starts. `None` leaves the stroke undragged. |
| `label(f)` | What the payload is called, read as the drag starts, for whoever draws the label beside the pointer. |
| `threshold(px)` | How far the pointer travels before a press is a drag rather than a click. 4 px unless set. |
| `on_detach(f)` | Told the payload and where on the surface it was let go, when it is let go over no area that accepts it. |
| `along(axis)` | The payload is carried along `axis` only: across it, the pointer counts as staying where it picked it up. |
| `within(f)` | The payload is carried inside the rect `f` answers, in the space the item is laid out in, read at each move. |

The item only starts the stroke. Once a press on it travels past its threshold, the session holds the stroke
itself: it hears every move and the release ahead of the widget tree, whichever box the pointer is over, until the
payload is let go or the drag is cancelled. So the item may go away mid-stroke and the payload is still carried to
where it is let go: a row of a list that builds only the rows on screen is dropped once the list scrolls it out of
view under a held pointer, and a tree's row carried to the far end of the tree is one. `carried_from()` keeps
naming the box the drag started on, so a widget that builds its items again knows its own payload by a key of its
own rather than by the box.

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

While a payload is carried the session watches every area: its box as layout places it, and whatever signals its
`layout` reads. When either changes, the pointer is measured again where it stands, so what it is over follows
the areas moving under a pointer that does not: the rows a folder opened by `on_hold` pushes down, a row the app
added, a list grown under the pointer. Reading the rows' signals with `get` in `layout` is what lets the session
see them change before the next layout pass; one read with `peek` is measured again once the area's box is laid
out anew.

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
readable once the payload has left the scroll area that clips its item. A tree does the same for its rows, with
the line set in to the level of the nodes the gap lies among, and always shows its own row's label.

## A tree

`tree_view` takes part in a session through its `drag` prop, a `TreeDrag` made of the session and three of the
app's functions over its own types: what a row carries, whether a payload may land at a place, and what to do when
one did.

```rust
use telar_components::{TreeDrag, TreeNode, TreeTarget, TreeViewProps, tree_view};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum LibraryNode {
    Folder(FolderId),
    Text(TextId),
    Section(SectionId),
}

let drag = TreeDrag::new(
    &session,
    |node: &TreeNode<LibraryNode>| Some(Carried::Node(node.id)),
    |carried: &Carried, target: &TreeTarget<LibraryNode>| match (carried, target) {
        (Carried::Segment(_), TreeTarget::Inside(LibraryNode::Section(_))) => true,
        (Carried::Node(LibraryNode::Text(_)), TreeTarget::Inside(LibraryNode::Folder(_))) => true,
        (Carried::Node(_), TreeTarget::Between { .. }) => true,
        _ => false,
    },
    move |carried, target| move_to(carried, target),
);
let tree = tree_view(TreeViewProps::props().items(nodes).drag(drag).build())?;
```

A place is reported as a `TreeTarget` in the tree's keys, never as a row of what it happens to show:
`Inside(key)` onto a node, a branch to go into or a leaf that takes what is dropped on it, and
`Between { parent, after, before }` for the gap among the nodes under `parent` (`None` for the top level), after
the sibling `after` and before the sibling `before`, `None` at either end. The gap before a row is among that row's
siblings, and the gap after the last row is at the end of the top level; dropping onto a branch is how a node goes
in after what the branch holds.

The tree refuses one thing itself: a node of its own into itself or anything under it. A node let go in the gap
it came out of is not reported. Held over a shut branch, the pointer opens it, and the rows it shows are measured
again under the pointer. A branch is any node built `with_children`, an empty list included, so an empty folder
opens and is dropped into like any other.

## A long list

`ReorderGroup::zone` builds every item it holds. A list of thousands, such as the sentences of a long text, is
one `virtual_zone` instead: it builds only the items its scroll shows, and a margin past each edge, and places
the rest where its `VirtualRows` says they stand, so the page costs what a screenful does whatever its length.

```rust
use telar::{DragSession, DropPlace, LayoutScrollArea, LayoutStyle, VirtualRows, memo};
use telar_components::{ReorderGroup, Slot, VirtualZoneProps};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Line {
    Heading(SectionId),
    Sentence(SegmentId),
}

// The text as one list: each section's heading, then its sentences unless it is folded.
let lines = memo(move || lines_of(&sections.get(), &folded.get()));
let line_at = move |slot: Slot| lines.with(|lines| lines.get(slot.index).copied());

let group = ReorderGroup::new(&session, move |slot| match line_at(slot) {
    Some(Line::Sentence(id)) => Some(Carried::Segment(id)),
    _ => None,
})
.accepts(move |_, place| match place {
    DropPlace::Between(to) => to.index > 0,
    DropPlace::Inside(onto) => matches!(line_at(onto), Some(Line::Heading(_))),
})
.on_drop(move |carried, place| move_to(carried, &lines.get(), place));

let page = LayoutScrollArea::new_with(LayoutStyle::column().flex_grow(1.0), move |viewport| {
    let zone = group.virtual_zone(
        VirtualZoneProps::props()
            .items(lines)
            .item(Rc::new(move |_, line| match line {
                Line::Heading(id) => section_heading(id),
                Line::Sentence(id) => sentence_row(id),
            }))
            .rows(VirtualRows::new(viewport, 48.0))
            .build(),
    )?;
    title_then(zone)
})?;
```

The design is one flat list rather than a zone per section: a section's heading and its sentences are items of
the same zone, told apart by the key the caller gives each. That is what lets one list build only what is on
screen across every section, and it is all a sectioned list needs:

- **Headings stay where they are.** The group's `payload` answers `None` for a heading, so the pointer does not
  pick it up and Alt+↑ and Alt+↓ pass over it.
- **Where a drop lands is the caller's to read.** A gap is counted in the list as it was shown when the item was
  let go, and the line before it says where it goes: after a sentence, or first in the section whose heading it
  follows. `accepts` refuses the gap before the first heading, and takes a sentence onto a heading, which is how a
  sentence goes into a folded section.
- **Folding is the list changing.** A folded section's sentences leave `items`, and come back when it opens; the
  measured height of every item is remembered by its key, so the rows after a section that folds or opens stay
  where they were measured.

Every item is a drop target and a step of the keyboard whether it is built or not. An item carried out of the
built window is still carried, and lands where it is let go: the group knows its own item by the press that
picked it up, not by the box. Alt+↓ on the last item in view moves it one gap down and brings it into view, and
focus follows it. Each built item says where it stands among all of them (`SetPosition`), so a reader counts the
whole list, not the screenful. `on_activate`, `label`, `on_detach`, `accepts`, `reach` and drops from other widgets
of the session work as they do on a zone that builds everything.

The zone may sit below anything else in its scroll, a title and its buttons included: the rows are counted from
the list's own top in the content. Items are stacked as they come, so the space between two sentences belongs
inside what `item` builds. The `rows` handle is the zone's own, and brings any item into view by its place
(`rows.reveal(index, margin)`), built or not.

## Scrolling near an edge

While a payload is carried, a scroll area that holds one of the session's areas scrolls when the pointer is
within 40 px of an edge it can still scroll towards (or a quarter of the area, when that is less), faster the
nearer the pointer is to the edge, up to 1200 px a second. The innermost such scroll area scrolls first. It
runs on the timer clock rather than on animation frames, so it is not shortened by the reduced-motion
preference, and it stops at the end of the content, when the pointer leaves the edge and when the stroke ends.
After each step the pointer is measured again against what scrolled under it.

## Ending a drag

Letting go drops the payload on the area under the pointer, or tells the item it was let go over none. Escape,
another button pressed mid-stroke or the window losing focus cancels it: the session is back as it was before
the press, nothing is dropped, and the scroll and the hold stop. A press that carried nothing yet, one that never
travelled past its item's threshold, is a click on the item.

## The keyboard

A session is the pointer's. A widget moves its items with keys of its own and reports the move the way a drop
is reported, so the app has one place to act on either. `ReorderGroup` steps the focused item one gap earlier or
later with Alt+↑ and Alt+↓. Its `on_activate(f)` is told which item a click or Enter opened; without it the group
takes no other key, so Enter, Space and the bare arrows stay the app's. A tree walks only on bare arrows, Home,
End, Page Up and Page Down, and asks its `on_key` about every key first, so Alt+↑ and Alt+↓ moving a node are the
app's to write and report as a drop would be.

## Targets

The same on every target: the session works from the pointer events every target delivers and from the layout
every target shares.

| Target | Notes |
| --- | --- |
| web-dom | A scroll area whose content the document scrolls is asked for its offset rather than set, so the areas are measured against the scrolled content once the document reports where it put it. |
| Android, touch on any target | A press on an item inside a scroll area is taken by the item once it travels past the threshold, as on a desktop: the finger drags the item, not the list. |
| TUI | The same, on the cell grid. |
| headless | The same. `telar::testing::route` with presses and moves drives a stroke, and `advance_time` lets the hold delay and the scroll steps pass; a play's `drag` carries an item in one step. The session hears a stroke through the overlay dispatch every runner and `route` call before the tree, so events handed straight to a box's `on_event` pick a payload up and never carry it. |

The grabbing cursor is the item's: once a row the session carries has scrolled out of the rows built, the pointer
shows the cursor of whatever it is over until the payload is let go.
