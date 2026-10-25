//! What a screen reader is told about the window right now.
//!
//! Telar had the whole of *operating* an interface without a mouse — a tab order scoped to what is genuinely reachable, arrow keys and type-ahead in every list, a focus ring that knows a tap from a Tab — and none of it reaches someone who cannot see the screen. That is a different question: not *where do the keys go* but *what is this and what state is it in*.
//!
//! Almost nothing here is authored per widget, and that is the design. The set of controls is the focus registry's own tab order, so the reader and the keyboard cannot come to different conclusions about what is on screen. The name is taken from **the text the widget actually draws inside itself**, which is how it reads to everyone else and needs no second copy that can fall out of step with the first. What an application does author — a name, a language, a box a reader skips — is resolved along the frame's own element nesting, the same nesting a document backend turns into elements.

use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use geometry_core::Rect;
use layout_core::NodeId;
use platform_core::{
    AccessNode, CurrentKind, Event, Key, ModifiersState, NamedKey, Role, SetPosition,
};
use renderer_core::DrawCommand;
use rustc_hash::{FxHashMap, FxHashSet};

use reactive_core::SurfaceHandle;

use crate::focus;
use crate::input_region::{hit_rect, visible_rect};

/// Whether the snapshot reads `command`, for a runner that keeps the last frame's reading between frames rather than the whole frame.
///
/// The matrices are read too: a raster target draws a text in its own box's space and moves it into place with one, so without them every label sat at the window's corner and no control was named by what it draws.
pub fn is_read(command: &DrawCommand) -> bool {
    matches!(
        command,
        DrawCommand::Text { .. }
            | DrawCommand::PushMatrix { .. }
            | DrawCommand::PopMatrix
            | DrawCommand::Image { .. }
            | DrawCommand::Path { .. }
            | DrawCommand::PushElement { .. }
            | DrawCommand::PopElement
            | DrawCommand::PushSurface { .. }
            | DrawCommand::PopSurface
            | DrawCommand::PushClip { .. }
            | DrawCommand::PopClip
    )
}

/// Everything the platform's accessibility layer needs to describe the window: a tree, listed in reading order, as [`AccessNode`] says.
///
/// `commands` is the frame the renderer is about to draw — the same list, so what is announced and what is painted are the same picture by construction rather than by agreement.
///
/// A surface drawn inside this one's frame ([`DrawCommand::PushSurface`]) is read in its own world, where its boxes' ids mean its boxes, or, for a [foreign](crate::ForeignSurface) one, taken as it reports itself; either way its nodes are placed where the frame shows them, inside the node it is drawn in: see [`NestedSurface::place`] and [`graft`].
pub fn snapshot(commands: &[DrawCommand]) -> Vec<AccessNode> {
    let SurfaceTree { nodes, nested } = surface_tree(commands);
    let grafts = nested
        .into_iter()
        .filter_map(|(surface, parent)| {
            let inner = match surface.foreign() {
                Some(foreign) => foreign.access_snapshot(),
                None => surface.with_entered(|| snapshot(&commands[surface.commands.clone()]))?,
            };
            Some((
                parent,
                inner.into_iter().map(|node| surface.place(node)).collect(),
            ))
        })
        .collect();
    graft(nodes, grafts)
}

/// What a reader is told about the surface entered now, which drew `commands`, leaving out every surface nested in its frame: [`snapshot`] for one surface, as a check that reads each surface in its own world wants it.
pub fn surface_snapshot(commands: &[DrawCommand]) -> Vec<AccessNode> {
    surface_tree(commands).nodes
}

/// One surface as a reader meets it, with the places the surfaces nested in its frame take in it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SurfaceTree {
    /// The [`surface_snapshot`].
    pub nodes: Vec<AccessNode>,
    /// The surfaces drawn directly inside the frame, in draw order, each with the node of [`nodes`](Self::nodes) it is drawn inside: `None` for one the window holds directly.
    pub nested: Vec<(NestedSurface, Option<usize>)>,
}

/// [`surface_snapshot`], and where in it each surface nested in the frame sits, for a caller reading those in their own worlds and joining them to it with [`graft`].
///
/// A node sits in the nearest box around it, by the frame's own element nesting, that is a node itself: a control, a box the application named, one that says where it sits, or one whose role is a structure a reader steps into, such as a tab list or a dialog. Such a box given no name is a node only while it holds one.
pub fn surface_tree(commands: &[DrawCommand]) -> SurfaceTree {
    let exposed = focus::exposed();
    let control_nodes: FxHashSet<NodeId> = exposed.iter().map(|e| e.node).collect();
    let reading = Reading::of(commands, &control_nodes);
    let controls: Vec<(focus::Exposed, Rect)> = exposed
        .into_iter()
        .filter(|e| !reading.scope(e.node).hidden)
        .filter_map(|e| visible_rect(e.node).map(|rect| (e, rect)))
        .collect();
    let focused = focus::current();
    let layers = focus::ModalLayers::now();
    let modal = |role: Role, node: NodeId| role == Role::Dialog && layers.holds(node);

    let mut named_controls = vec![false; controls.len()];
    let mut found: Vec<Found> = controls
        .iter()
        .enumerate()
        .map(|(i, (e, rect))| {
            let label = reading.label_of(e.node);
            named_controls[i] = label.is_some();
            let link = reading.links.get(&e.node);
            let node = AccessNode {
                id: Some(e.id),
                parent: None,
                role: e.role,
                name: label.map(str::to_string).unwrap_or_default(),
                rect: hit_rect(e.node).unwrap_or(*rect),
                focused: focused == Some(e.id),
                enabled: e.enabled,
                toggled: e.toggled,
                value: e.value,
                orientation: e.orientation,
                expanded: e.expanded,
                has_popup: e.has_popup,
                keyboard_shortcut: e.keyboard_shortcut.as_deref().map(str::to_string),
                sort: e.sort,
                multiselectable: e.multiselectable,
                position: e.position,
                lang: reading.scope(e.node).lang.as_deref().map(str::to_string),
                url: link.map(|(destination, _)| platform_core::address_of(destination)),
                current: link.and_then(|(_, current)| *current),
                active_descendant: focus::active_descendant_state(e.id)
                    .filter(|item| controls.iter().any(|(c, _)| c.id == *item)),
                controls: focus::controls_state(e.id)
                    .filter(|item| controls.iter().any(|(c, _)| c.id == *item)),
                modal: modal(e.role, e.node),
            };
            Found::holding(node, e.node, &reading)
        })
        .collect();

    let is_control = |node: NodeId| controls.iter().any(|(e, _)| e.node == node);
    let words_of_title = |named: &Named| {
        named
            .titled
            .filter(|_| named.role == Role::Group && wrapped_control(named, &controls).is_none())
    };
    let mut titles: Vec<Vec<(usize, &str)>> = reading
        .titled
        .iter()
        .map(|titled| {
            titled
                .words
                .iter()
                .map(|(at, word)| (*at, word.as_str()))
                .collect()
        })
        .collect();
    for named in reading.named.iter().filter(|named| !is_control(named.node)) {
        if let Some(title) = words_of_title(named) {
            titles[title].push((named.opened_at, &named.label));
        }
    }
    let pieces = reading
        .named
        .iter()
        .filter(|named| !is_control(named.node))
        .filter(|named| words_of_title(named).is_none() || named.is_picture())
        .filter_map(|named| {
            let rect = visible_rect(named.node).filter(|rect| area(*rect) > 0.0)?;
            let role = match named.role {
                _ if named.is_picture() => Role::Drawing,
                Role::Group => Role::Label,
                role => role,
            };
            Some(Piece {
                text: named.label.to_string(),
                rect,
                role,
                lang: named.lang.clone(),
                url: None,
                position: named.position,
                named_box: Some(NamedBox {
                    node: named.node,
                    wraps: wrapped_control(named, &controls),
                }),
                drawn_at: Some(named.opened_at),
                drawn_in: Some(named.node),
                control: None,
            })
        })
        .chain(reading.text.iter().cloned())
        .filter(|piece| !piece.drawn_in.is_some_and(|node| layers.behind(node)));

    for piece in pieces {
        let owner = match &piece.named_box {
            Some(NamedBox { wraps: Some(i), .. }) => Some(*i),
            // The control it is drawn inside, or failing that the smallest one whose box contains it, so a button inside a card is named by its own label.
            _ => piece
                .control
                .and_then(|node| {
                    controls
                        .iter()
                        .position(|(control, _)| control.node == node)
                })
                .or_else(|| {
                    controls
                        .iter()
                        .enumerate()
                        .filter(|(_, (_, bounds))| contains(*bounds, piece.rect))
                        .filter(|(_, (control, _))| !piece.groups(control.node))
                        .filter(|(_, (control, _))| !piece.lies_under(control.node, &reading))
                        .min_by(|(_, (_, a)), (_, (_, b))| area(*a).total_cmp(&area(*b)))
                        .map(|(i, _)| i)
                }),
        };
        match owner {
            // A control the application named is called that, whatever it draws.
            Some(i) if named_controls[i] && piece.url.is_none() => {}
            Some(i) if piece.url.is_none() => append(&mut found[i].node.name, &piece.text),
            // Text belonging to no control is still content: a heading, a caption, the paragraph a dialog asks about. A link run is its own node wherever it sits.
            _ => {
                let node = AccessNode {
                    lang: piece.lang.as_deref().map(str::to_string),
                    url: piece.url,
                    position: piece.position,
                    modal: piece.drawn_in.is_some_and(|node| modal(piece.role, node)),
                    ..unaddressed(piece.role, piece.text, piece.rect)
                };
                found.push(match piece.named_box {
                    Some(named) if holds_what_is_inside(node.role) => {
                        Found::holding(node, named.node, &reading)
                    }
                    _ => Found::inside(node, piece.drawn_in, &reading),
                });
            }
        }
    }

    found.extend(
        reading
            .placed
            .iter()
            .filter(|placed| !layers.behind(placed.node))
            .filter_map(|placed| {
                let rect = visible_rect(placed.node).filter(|rect| area(*rect) > 0.0)?;
                let node = AccessNode {
                    position: Some(placed.position),
                    ..unaddressed(placed.role, String::new(), rect)
                };
                Some(Found::holding(node, placed.node, &reading))
            }),
    );

    found.extend(
        reading
            .structures
            .iter()
            .filter(|structure| !layers.behind(structure.node))
            .filter_map(|structure| {
                let rect = visible_rect(structure.node).filter(|rect| area(*rect) > 0.0)?;
                let node = AccessNode {
                    modal: modal(structure.role, structure.node),
                    ..unaddressed(structure.role, String::new(), rect)
                };
                Some(Found::holding(node, structure.node, &reading))
            }),
    );

    found.extend(
        reading
            .titled
            .iter()
            .zip(titles)
            .filter(|(titled, _)| !layers.behind(titled.node))
            .filter_map(|(titled, mut words)| {
                let rect = visible_rect(titled.node).filter(|rect| area(*rect) > 0.0)?;
                words.sort_by_key(|(at, _)| *at);
                let mut name = String::new();
                for (_, word) in words {
                    append(&mut name, word);
                }
                let node = AccessNode {
                    lang: titled.lang.as_deref().map(str::to_string),
                    ..unaddressed(titled.role, name, rect)
                };
                Some(Found::holding(node, titled.node, &reading))
            }),
    );

    let surfaces = nested_surfaces(commands);
    let (nodes, anchors) = arrange(found, &reading, &reading.surfaces_in);
    SurfaceTree {
        nodes,
        nested: surfaces.into_iter().zip(anchors).collect(),
    }
}

/// A node nothing addresses and that carries no state, as text, a named box and a placed box begin before saying what sets each apart.
fn unaddressed(role: Role, name: String, rect: Rect) -> AccessNode {
    AccessNode {
        id: None,
        parent: None,
        role,
        name,
        rect,
        focused: false,
        enabled: true,
        toggled: None,
        value: None,
        orientation: None,
        expanded: None,
        position: None,
        lang: None,
        url: None,
        current: None,
        active_descendant: None,
        controls: None,
        has_popup: None,
        keyboard_shortcut: None,
        sort: None,
        multiselectable: false,
        modal: false,
    }
}

/// Whether a node read for a box holds the nodes read inside that box. Text, and a picture, are read as what they say; a box named as a group reads as its name, a caption beside what it groups.
fn holds_what_is_inside(role: Role) -> bool {
    !matches!(role, Role::Label | Role::Drawing)
}

/// Whether a box given no name is still a node for the nodes inside it, as the element a document makes of it is: a tab list around its tabs, a dialog around what it asks, a landmark around its region. A plain group is scaffolding, a heading is named by its text (see [`is_named_by_content`]), and a section or a form with no name is no landmark.
fn holds_structure(role: Role) -> bool {
    matches!(
        role,
        Role::TabList
            | Role::TabPanel
            | Role::Menu
            | Role::ListBox
            | Role::Tree
            | Role::Grid
            | Role::Row
            | Role::Toolbar
            | Role::List
            | Role::Dialog
            | Role::Status
            | Role::Log
            | Role::Banner
            | Role::Navigation
            | Role::Main
            | Role::Complementary
            | Role::ContentInfo
            | Role::Article
            | Role::Search
    )
}

/// Whether a box given no name is a node for the nodes inside it because the application said what it is, as a component call's `role:` does: a role said on purpose is never scaffolding, so a list item, a section or a form said that way is read as one wherever a document makes it one, unlike a plain wrapper the widget gave that role. A plain group says nothing, said or not.
fn said_as_structure(role: Role, said: Option<Role>) -> bool {
    said == Some(role) && role != Role::Group && role.is_annotatable()
}

/// Whether a box given no name is named by the words drawn inside it, as a document's `<h2>` is by its text: a heading, read as one node of its role and level rather than as the plain text it holds.
fn is_named_by_content(role: Role) -> bool {
    matches!(role, Role::Heading(_))
}

/// A node on its way into the tree: the box it was read for, if it holds what is inside that box, where in the frame's nesting it sits, and where it falls in reading order.
struct Found {
    node: AccessNode,
    holds: Option<NodeId>,
    /// The innermost box around it: for a node read for a box, the box's parent; for text, the box it is drawn in.
    within: Option<NodeId>,
    /// Its rect where the flow puts it, which a box stuck to the edge of a scrolled view is read by among its siblings: the header row of a grid is read before the rows scrolled under it.
    key: Rect,
}

impl Found {
    fn holding(node: AccessNode, at: NodeId, reading: &Reading) -> Self {
        Self {
            key: reading.in_flow(node.rect, Some(at)),
            within: reading.parent_of(at),
            holds: Some(at),
            node,
        }
    }

    fn inside(node: AccessNode, at: Option<NodeId>, reading: &Reading) -> Self {
        Self {
            key: reading.in_flow(node.rect, at),
            within: at,
            holds: None,
            node,
        }
    }

    /// Whether a reader is told about it for what it says itself, rather than only for the nodes it holds.
    fn speaks(&self) -> bool {
        self.node.id.is_some() || !self.node.name.is_empty() || self.node.position.is_some()
    }
}

/// Joins `found` into a tree and lists it in reading order: each node in the nearest node around it, siblings by where they fall, and a node that says nothing itself kept only while it holds one that does. Also places each of `anchors`, a box of the frame, in the nearest node around it.
fn arrange(
    found: Vec<Found>,
    reading: &Reading,
    anchors: &[Option<NodeId>],
) -> (Vec<AccessNode>, Vec<Option<usize>>) {
    let mut holders: FxHashMap<NodeId, usize> = FxHashMap::default();
    for (i, f) in found.iter().enumerate() {
        if let Some(node) = f.holds {
            holders.entry(node).or_insert(i);
        }
    }
    let enclosing = |mut at: Option<NodeId>| {
        while let Some(node) = at {
            if let Some(&holder) = holders.get(&node) {
                return Some(holder);
            }
            at = reading.parent_of(node);
        }
        None
    };
    let parents: Vec<Option<usize>> = found.iter().map(|f| enclosing(f.within)).collect();

    let mut kept: Vec<bool> = found.iter().map(Found::speaks).collect();
    for i in 0..found.len() {
        if !kept[i] {
            continue;
        }
        let mut at = parents[i];
        while let Some(above) = at {
            if kept[above] {
                break;
            }
            kept[above] = true;
            at = parents[above];
        }
    }
    let kept_around = |mut at: Option<usize>| {
        while let Some(i) = at {
            if kept[i] {
                return Some(i);
            }
            at = parents[i];
        }
        None
    };

    let mut roots = Vec::new();
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); found.len()];
    for i in (0..found.len()).filter(|&i| kept[i]) {
        match kept_around(parents[i]) {
            Some(parent) => children[parent].push(i),
            None => roots.push(i),
        }
    }
    let by_key = |a: &usize, b: &usize| reading_order(found[*a].key, found[*b].key);
    roots.sort_by(by_key);
    for siblings in &mut children {
        siblings.sort_by(by_key);
    }

    let anchors = anchors
        .iter()
        .map(|anchor| kept_around(enclosing(*anchor)))
        .collect::<Vec<_>>();
    let (nodes, placed) = flatten(
        found.into_iter().map(|f| f.node).collect(),
        &roots,
        &children,
    );
    let anchors = anchors
        .into_iter()
        .map(|anchor| anchor.and_then(|i| placed[i]))
        .collect();
    (nodes, anchors)
}

/// Lists the tree `roots` and `children` make of `nodes` depth first, each node's [`parent`](AccessNode::parent) set to its parent's place in the list, and returns it with the place each of `nodes` took: `None` for one the tree does not reach.
fn flatten(
    nodes: Vec<AccessNode>,
    roots: &[usize],
    children: &[Vec<usize>],
) -> (Vec<AccessNode>, Vec<Option<usize>>) {
    let mut placed: Vec<Option<usize>> = vec![None; nodes.len()];
    let mut nodes: Vec<Option<AccessNode>> = nodes.into_iter().map(Some).collect();
    let mut listed = Vec::with_capacity(nodes.len());
    let mut pending: Vec<(usize, Option<usize>)> =
        roots.iter().rev().map(|&root| (root, None)).collect();
    while let Some((at, parent)) = pending.pop() {
        let Some(mut node) = nodes[at].take() else {
            continue;
        };
        node.parent = parent;
        placed[at] = Some(listed.len());
        pending.extend(
            children[at]
                .iter()
                .rev()
                .map(|&child| (child, Some(listed.len()))),
        );
        listed.push(node);
    }
    (listed, placed)
}

/// Joins to `nodes`, a snapshot, the snapshots in `grafts`, each under the node of `nodes` it names or at the top for `None`: how [`snapshot`] joins the surfaces nested in a frame to the frame's own tree.
///
/// The nodes already in `nodes` keep their order; a grafted snapshot's top nodes are read among the siblings they join by where they fall.
pub fn graft(
    nodes: Vec<AccessNode>,
    grafts: Vec<(Option<usize>, Vec<AccessNode>)>,
) -> Vec<AccessNode> {
    if grafts.iter().all(|(_, inner)| inner.is_empty()) {
        return nodes;
    }
    let mut all = nodes;
    let mut roots = Vec::new();
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); all.len()];
    for (i, node) in all.iter().enumerate() {
        match node.parent.filter(|&parent| parent < i) {
            Some(parent) => children[parent].push(i),
            None => roots.push(i),
        }
    }
    let outer = all.len();
    for (parent, inner) in grafts {
        let offset = all.len();
        let mut tops = Vec::new();
        for (k, node) in inner.into_iter().enumerate() {
            children.push(Vec::new());
            match node.parent.filter(|&above| above < k) {
                Some(above) => children[offset + above].push(offset + k),
                None => tops.push(offset + k),
            }
            all.push(node);
        }
        let siblings = match parent.filter(|&parent| parent < outer) {
            Some(parent) => &mut children[parent],
            None => &mut roots,
        };
        *siblings = merge(std::mem::take(siblings), tops, |a, b| {
            reading_order(all[a].rect, all[b].rect)
        });
    }
    flatten(all, &roots, &children).0
}

/// `a` and `b`, each already in order, as one list in order: of two that compare equal, `a`'s first.
fn merge(
    a: Vec<usize>,
    b: Vec<usize>,
    order: impl Fn(usize, usize) -> std::cmp::Ordering,
) -> Vec<usize> {
    let mut merged = Vec::with_capacity(a.len() + b.len());
    let (mut a, mut b) = (a.into_iter().peekable(), b.into_iter().peekable());
    loop {
        let next = match (a.peek(), b.peek()) {
            (Some(&x), Some(&y)) if order(y, x).is_lt() => b.next(),
            (Some(_), _) => a.next(),
            (None, _) => b.next(),
        };
        match next {
            Some(i) => merged.push(i),
            None => return merged,
        }
    }
}

/// Something a reader lands on that is not a control of its own: a run of text, or a box the application named.
#[derive(Clone)]
struct Piece {
    text: String,
    rect: Rect,
    role: Role,
    lang: Option<Arc<str>>,
    /// Where a link run of a paragraph goes, as the address a reader announces.
    url: Option<String>,
    /// Where a named box sits among the items of its set, as its role says it.
    position: Option<SetPosition>,
    /// The box the application named, for a piece that is one rather than a run of text.
    named_box: Option<NamedBox>,
    /// Where in the frame a run of text was drawn, or a named box began.
    drawn_at: Option<usize>,
    /// The innermost box a run of text was drawn in, or the named box itself.
    drawn_in: Option<NodeId>,
    /// The innermost control a run of text was drawn inside, which names it whatever scrolling or clipping has done to where it shows.
    control: Option<NodeId>,
}

#[derive(Clone)]
struct NamedBox {
    node: NodeId,
    /// The control, by its place among the snapshot's controls, that the box only wraps and whose name it therefore is.
    wraps: Option<usize>,
}

impl Piece {
    /// Whether this is a named box around the control at `node` that names a group the control is in, such as a tab list, and never the control itself.
    fn groups(&self, node: NodeId) -> bool {
        self.named_box
            .as_ref()
            .is_some_and(|named| crate::input_region::is_inside(node, named.node))
    }

    /// Whether this is a run of text, or a named box, painted before the control at `node` began, outside it: the control is drawn over it, as a menu is over the page it opens on, so it is covered rather than a label.
    fn lies_under(&self, node: NodeId, reading: &Reading) -> bool {
        self.drawn_at
            .zip(reading.controls_opened.get(&node))
            .is_some_and(|(drawn, opened)| drawn < *opened)
    }
}

/// The control a named box is no more than a wrapper around, as `<label>` is around its field: the one control inside it, when the box has no role of its own and draws nothing beside that control. A box with a role, such as a tab list or a navigation, or one holding several controls or content of its own, names itself.
fn wrapped_control(named: &Named, controls: &[(focus::Exposed, Rect)]) -> Option<usize> {
    let inside: Vec<usize> = controls
        .iter()
        .enumerate()
        .filter(|(_, (control, _))| crate::input_region::is_inside(control.node, named.node))
        .map(|(i, _)| i)
        .take(2)
        .collect();
    named
        .role
        .lends_name_to_control(named.drew_beside_controls, inside.len())
        .then(|| inside[0])
}

/// What an element's ancestors said that reaches it.
#[derive(Clone, Default)]
struct Scope {
    hidden: bool,
    lang: Option<Arc<str>>,
}

/// A box the application gave a name to, and what it drew that a reader would otherwise have found.
struct Named {
    node: NodeId,
    label: Arc<str>,
    lang: Option<Arc<str>>,
    /// What the box said it is; [`Role::Group`] for a box with no role of its own.
    role: Role,
    /// Where it sits among the items of its set, as its role says it.
    position: Option<SetPosition>,
    /// The named box this one sits in, by its place in [`Reading::named`].
    enclosing: Option<usize>,
    /// The box named by its content that this one sits directly in, by its place in [`Reading::titled`], whose name this one's is part of when it has no role of its own.
    titled: Option<usize>,
    /// Where in the frame the box began.
    opened_at: usize,
    drew_text: bool,
    drew_art: bool,
    /// Whether it drew words or artwork outside every control inside it.
    drew_beside_controls: bool,
}

/// A box given no name that says where it sits among items a reader cannot count, such as the header row of a grid whose rows are not all built. Its place is what it has to say, so it is a node of its own; the controls and the text inside it are read inside it as they would be without it, and none of them becomes its name.
struct Placed {
    node: NodeId,
    role: Role,
    position: SetPosition,
}

/// A box given no name that a reader names by the words drawn in it, in draw order: see [`is_named_by_content`].
struct Titled {
    node: NodeId,
    role: Role,
    lang: Option<Arc<str>>,
    words: Vec<(usize, String)>,
}

/// A box given no name whose role is a structure a reader steps into: see [`holds_structure`].
struct Structure {
    node: NodeId,
    role: Role,
}

impl Named {
    /// A named box that draws artwork and no words is a picture; anything else reads as the text it stands for.
    fn is_picture(&self) -> bool {
        self.drew_art && !self.drew_text
    }
}

/// What a reader makes of one box a frame draws, as [`FrameReading`] reports it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BoxReading {
    /// A reader skips it and everything in it: it, or a box around it, is hidden from readers.
    pub skipped: bool,
    /// The name the application gave it. `None` for a box given none, and for one a reader skips, which is never announced by any name.
    pub name: Option<Arc<str>>,
    /// The language it is in: the nearest said on it or around it.
    pub lang: Option<Arc<str>>,
}

/// One bitmap or piece of vector art a frame draws, as a reader meets it.
#[derive(Clone, Debug, PartialEq)]
pub struct Artwork {
    /// Its command's place in the frame.
    pub index: usize,
    /// The innermost box it is drawn in; `None` for art drawn outside every box.
    pub node: Option<NodeId>,
    /// Whether it is drawn inside a control, which a reader announces by the control's name instead.
    pub in_control: bool,
    /// A reader skips it: a box it is drawn in is hidden from readers.
    pub skipped: bool,
    /// The name a reader hears for it: that of the innermost named box around it, when that box draws artwork and no words and so reads as the picture. `None` for art a reader skips.
    pub name: Option<Arc<str>>,
}

/// A frame as a reader meets it, box by box and picture by picture: the annotations [`snapshot`] resolves, kept where it folds them away, for a checker asking what a reader skips and what it hears for each box.
///
/// Read with the frame's surface entered, as [`snapshot`] is: the annotations and the controls are the surface's own.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FrameReading {
    boxes: FxHashMap<NodeId, BoxReading>,
    artwork: Vec<Artwork>,
}

impl FrameReading {
    pub fn of(commands: &[DrawCommand]) -> Self {
        let controls: FxHashSet<NodeId> = focus::exposed()
            .iter()
            .map(|e| e.node)
            .chain(focus::control_nodes())
            .collect();
        let reading = Reading::of(commands, &controls);
        let boxes = reading
            .scopes
            .iter()
            .map(|(node, scope)| {
                let box_reading = BoxReading {
                    skipped: scope.hidden,
                    name: reading.label_of(*node).map(Arc::from),
                    lang: scope.lang.clone(),
                };
                (*node, box_reading)
            })
            .collect();
        let artwork = reading
            .artwork
            .iter()
            .map(|seen| Artwork {
                index: seen.index,
                node: seen.node,
                in_control: seen.in_control,
                skipped: seen.skipped,
                name: seen
                    .named
                    .filter(|_| !seen.skipped)
                    .map(|i| &reading.named[i])
                    .filter(|named| named.is_picture())
                    .map(|named| Arc::clone(&named.label)),
            })
            .collect();
        Self { boxes, artwork }
    }

    /// What a reader makes of `node`, or `None` for a box the frame does not draw.
    pub fn get(&self, node: NodeId) -> Option<&BoxReading> {
        self.boxes.get(&node)
    }

    /// Whether a reader skips `node`. A box the frame does not draw is not skipped: nothing hid it.
    pub fn skips(&self, node: NodeId) -> bool {
        self.get(node).is_some_and(|reading| reading.skipped)
    }

    /// Every box the frame draws, in no particular order.
    pub fn boxes(&self) -> impl Iterator<Item = (NodeId, &BoxReading)> {
        self.boxes.iter().map(|(node, reading)| (*node, reading))
    }

    /// Every bitmap and piece of vector art the frame draws, in draw order.
    pub fn artwork(&self) -> &[Artwork] {
        &self.artwork
    }
}

/// Art as the walk meets it, before the boxes around it have finished saying what they drew.
struct SeenArt {
    index: usize,
    node: Option<NodeId>,
    in_control: bool,
    skipped: bool,
    /// The innermost named box it is drawn in, by its place in [`Reading::named`].
    named: Option<usize>,
}

/// One open element, as the walk carries it.
#[derive(Clone, Default)]
struct Open {
    scope: Scope,
    /// The innermost named box, by its place in [`Reading::named`].
    named: Option<usize>,
    /// The box named by its content that the element sits in with no named box between, by its place in [`Reading::titled`].
    titled: Option<usize>,
    control: Option<NodeId>,
    node: Option<NodeId>,
    /// How far the element and the boxes around it are moved from their places in the flow by sticking, on screen.
    shift: (f32, f32),
}

/// One frame, as a reader meets it: the annotations resolved along its element nesting, with everything under a hidden box already gone.
#[derive(Default)]
struct Reading {
    scopes: FxHashMap<NodeId, Scope>,
    named: Vec<Named>,
    placed: Vec<Placed>,
    structures: Vec<Structure>,
    titled: Vec<Titled>,
    text: Vec<Piece>,
    artwork: Vec<SeenArt>,
    /// Where in the frame each control's box began.
    controls_opened: FxHashMap<NodeId, usize>,
    /// Where each link box goes, and what it is the current one of when it is marked so.
    links: FxHashMap<NodeId, (platform_core::Destination, Option<CurrentKind>)>,
    /// The element each element is drawn inside, where it is drawn inside one.
    parents: FxHashMap<NodeId, NodeId>,
    /// How far sticking moves each element that it moves, on screen, with the boxes around it: see [`Reading::in_flow`].
    shifts: FxHashMap<NodeId, (f32, f32)>,
    /// The innermost element around each surface drawn directly inside the frame, in draw order.
    surfaces_in: Vec<Option<NodeId>>,
}

impl Reading {
    fn of(commands: &[DrawCommand], controls: &FxHashSet<NodeId>) -> Self {
        let mut reading = Self::default();
        let mut open: Vec<Open> = Vec::new();
        let mut index = 0usize;
        let mut nested = 0usize;
        let mut clipped_away: Vec<bool> = Vec::new();
        renderer_core::for_each_with_matrix(commands, |command, matrix| {
            let at = index;
            index += 1;
            // A nested surface's boxes are numbered in its own world, where they are read; here their ids would name this surface's boxes.
            match command {
                DrawCommand::PushSurface { .. } => {
                    if nested == 0 {
                        reading
                            .surfaces_in
                            .push(open.last().and_then(|open| open.node));
                    }
                    nested += 1;
                    return;
                }
                DrawCommand::PopSurface => {
                    nested = nested.saturating_sub(1);
                    return;
                }
                _ if nested > 0 => return,
                _ => {}
            }
            let Open {
                scope,
                named,
                titled,
                control,
                node: innermost,
                shift: outer_shift,
            } = open.last().cloned().unwrap_or_default();
            let in_control = control.is_some();
            match command {
                DrawCommand::PushClip { rect, .. } => {
                    let placed = renderer_core::transform_clip_rect(matrix, *rect);
                    let empty = placed.width <= 0.0 || placed.height <= 0.0;
                    let outer = clipped_away.last().copied().unwrap_or(false);
                    clipped_away.push(outer || empty);
                }
                DrawCommand::PopClip => {
                    clipped_away.pop();
                }
                DrawCommand::PushElement { element } => {
                    let node = NodeId::from(element.id.0);
                    if controls.contains(&node) {
                        reading.controls_opened.entry(node).or_insert(at);
                    }
                    if let Some(link) = &element.semantics.link {
                        reading
                            .links
                            .insert(node, (link.clone(), element.semantics.current_kind()));
                    }
                    let annotation = crate::annotation::peek(node).unwrap_or_default();
                    let inner = Scope {
                        hidden: scope.hidden || annotation.hidden,
                        lang: annotation.lang.or(scope.lang),
                    };
                    let role = match controls.contains(&node) {
                        true => element.semantics.role,
                        false => element.semantics.role.annotated(annotation.role),
                    };
                    let mut inner_titled = titled;
                    let position = element
                        .semantics
                        .position
                        .filter(|_| role.positioning().is_some());
                    if annotation.label.is_none() && !inner.hidden && !controls.contains(&node) {
                        match position {
                            Some(position) => reading.placed.push(Placed {
                                node,
                                role,
                                position,
                            }),
                            None if holds_structure(role) => {
                                reading.structures.push(Structure { node, role })
                            }
                            None if is_named_by_content(role) => {
                                reading.titled.push(Titled {
                                    node,
                                    role,
                                    lang: inner.lang.clone(),
                                    words: Vec::new(),
                                });
                                inner_titled = Some(reading.titled.len() - 1);
                            }
                            None if said_as_structure(role, annotation.role) => {
                                reading.structures.push(Structure { node, role })
                            }
                            None => {}
                        }
                    }
                    if let Some(parent) = innermost {
                        reading.parents.entry(node).or_insert(parent);
                    }
                    let (dx, dy) = layout_reactive::sticky_displacement(node);
                    let [a, b, c, d, ..] = matrix;
                    let shift = (
                        outer_shift.0 + a * dx + c * dy,
                        outer_shift.1 + b * dx + d * dy,
                    );
                    if shift != (0.0, 0.0) {
                        reading.shifts.insert(node, shift);
                    }
                    let named = match annotation.label {
                        Some(label) if !inner.hidden => {
                            reading.named.push(Named {
                                node,
                                label,
                                lang: inner.lang.clone(),
                                role,
                                position,
                                enclosing: named,
                                titled,
                                opened_at: at,
                                drew_text: false,
                                drew_art: false,
                                drew_beside_controls: false,
                            });
                            inner_titled = None;
                            Some(reading.named.len() - 1)
                        }
                        _ => named,
                    };
                    reading.scopes.insert(node, inner.clone());
                    open.push(Open {
                        scope: inner,
                        named,
                        titled: inner_titled,
                        control: if controls.contains(&node) {
                            Some(node)
                        } else {
                            control
                        },
                        node: Some(node),
                        shift,
                    });
                }
                DrawCommand::PopElement => {
                    open.pop();
                }
                DrawCommand::Image { .. } | DrawCommand::Path { .. } => {
                    reading.artwork.push(SeenArt {
                        index: at,
                        node: innermost,
                        in_control,
                        skipped: scope.hidden,
                        named,
                    });
                    if scope.hidden {
                        return;
                    }
                    if let Some(i) = named {
                        reading.named[i].drew_art = true;
                    }
                    if !in_control {
                        reading.drew_beside_controls(named);
                    }
                }
                _ if scope.hidden => {}
                DrawCommand::Text {
                    text, rect, spans, ..
                } if !text.trim().is_empty() => {
                    let placed = renderer_core::transform_clip_rect(matrix, *rect);
                    if let Some(i) = named {
                        reading.named[i].drew_text = true;
                    }
                    if !in_control {
                        reading.drew_beside_controls(named);
                    }
                    if !in_control && clipped_away.last().copied().unwrap_or(false) {
                        return;
                    }
                    match titled.filter(|_| !in_control) {
                        Some(title) => reading.titled[title].words.push((at, text.to_string())),
                        None => reading.text.push(Piece {
                            text: text.to_string(),
                            rect: placed,
                            role: Role::Label,
                            lang: scope.lang.clone(),
                            url: None,
                            position: None,
                            named_box: None,
                            drawn_at: Some(at),
                            drawn_in: innermost,
                            control,
                        }),
                    }
                    for span in spans.iter().flat_map(|spans| spans.iter()) {
                        let (Some(link), Some(words)) = (
                            &span.link,
                            text.get(span.range.start as usize..span.range.end as usize),
                        ) else {
                            continue;
                        };
                        reading.text.push(Piece {
                            text: words.to_string(),
                            rect: placed,
                            role: Role::Link,
                            lang: scope.lang.clone(),
                            url: Some(platform_core::address_of(link)),
                            position: None,
                            named_box: None,
                            drawn_at: Some(at),
                            drawn_in: innermost,
                            control,
                        });
                    }
                }
                _ => {}
            }
        });
        reading
    }

    /// Marks `innermost` and every named box around it as holding something besides its controls.
    fn drew_beside_controls(&mut self, innermost: Option<usize>) {
        let mut at = innermost;
        while let Some(i) = at {
            let named = &mut self.named[i];
            if named.drew_beside_controls {
                break;
            }
            named.drew_beside_controls = true;
            at = named.enclosing;
        }
    }

    /// What reaches `node`. A box the frame never drew says nothing, and nothing above it could either.
    fn scope(&self, node: NodeId) -> Scope {
        self.scopes.get(&node).cloned().unwrap_or_default()
    }

    fn label_of(&self, node: NodeId) -> Option<&str> {
        self.named
            .iter()
            .find(|named| named.node == node)
            .map(|named| &*named.label)
    }

    fn parent_of(&self, node: NodeId) -> Option<NodeId> {
        self.parents.get(&node).copied()
    }

    /// `rect`, of something drawn in the element `at`, where it would show if no box around it stuck: where it falls among the boxes beside it in the flow.
    fn in_flow(&self, rect: Rect, at: Option<NodeId>) -> Rect {
        let (dx, dy) = at
            .and_then(|node| self.shifts.get(&node).copied())
            .unwrap_or_default();
        Rect::new(rect.x - dx, rect.y - dy, rect.width, rect.height)
    }
}

/// Whether `inner`'s centre lies in `outer`. The centre and not the whole rect: a label clipped by its own control — a long menu item, a cell in a narrow column — still belongs to it.
fn contains(outer: Rect, inner: Rect) -> bool {
    let (x, y) = (inner.x + inner.width / 2.0, inner.y + inner.height / 2.0);
    x >= outer.x && x <= outer.x + outer.width && y >= outer.y && y <= outer.y + outer.height
}

fn area(rect: Rect) -> f32 {
    rect.width * rect.height
}

fn append(name: &mut String, piece: &str) {
    if !name.is_empty() {
        name.push(' ');
    }
    name.push_str(piece.trim());
}

/// Reading order rather than tab order, among the nodes one node holds: a label sits between the controls it explains. Tab order stays the focus registry's answer.
///
/// Of two nodes that begin at the same corner the larger is read first, as the one the other is drawn over.
fn reading_order(a: Rect, b: Rect) -> std::cmp::Ordering {
    a.y.total_cmp(&b.y)
        .then(a.x.total_cmp(&b.x))
        .then(area(b).total_cmp(&area(a)))
}

/// A surface drawn inside another's frame, as the [`DrawCommand::PushSurface`] marker around it names it: the commands it drew and where the frame around it shows them.
#[derive(Clone, Debug, PartialEq)]
pub struct NestedSurface {
    /// Its handle, as the marker carries it.
    pub surface: u64,
    /// Its own commands, between the markers, as a range of the frame they were found in. They are in its own coordinates.
    pub commands: Range<usize>,
    /// Maps a point in its coordinates to one in the frame's.
    pub placement: [f32; 6],
}

impl NestedSurface {
    /// Runs `f` with the surface's world active, as reading what it drew needs; `None` without running it once the surface is gone, when its boxes mean nothing on this thread.
    pub fn with_entered<R>(&self, f: impl FnOnce() -> R) -> Option<R> {
        SurfaceHandle::from_raw(self.surface).with_entered_if_alive(f)
    }

    /// Where the frame shows `rect`, given in the surface's coordinates.
    pub fn place_rect(&self, rect: Rect) -> Rect {
        renderer_core::transform_clip_rect(self.placement, rect)
    }

    /// The [`ForeignSurface`](crate::ForeignSurface) the marker names, which reports what is read of it instead of being entered; `None` for a surface of this process.
    pub fn foreign(&self) -> Option<Rc<dyn crate::ForeignSurface>> {
        crate::foreign_surface::source(self.surface)
    }

    /// A node read inside the surface, as the frame around it shows it: at its place in the frame, with its id and the ones it names as its active descendant and as what it controls made distinct from every id of the frame's own. An id of a foreign surface that names a surface nested in it no frame here draws any longer names nothing.
    pub fn place(&self, node: AccessNode) -> AccessNode {
        AccessNode {
            rect: self.place_rect(node.rect),
            id: node.id.and_then(|id| self.qualify(id)),
            active_descendant: node.active_descendant.and_then(|id| self.qualify(id)),
            controls: node.controls.and_then(|id| self.qualify(id)),
            ..node
        }
    }

    // Every surface counts its focus ids from the same start, so a nested one's are moved above the frame's; ids already moved by a surface nested deeper are unique as they are, since handles are, but a foreign surface's carry handles in its own numbering, which here name other surfaces.
    fn qualify(&self, id: u64) -> Option<u64> {
        match id >> NESTED_ID_SHIFT {
            0 => Some((self.surface << NESTED_ID_SHIFT) | id),
            guest if crate::foreign_surface::is_foreign(self.surface) => {
                crate::foreign_surface::nested_key_of(self.surface, guest)
                    .map(|key| (key << NESTED_ID_SHIFT) | (id & NESTED_ID_MASK))
            }
            _ => Some(id),
        }
    }
}

pub(crate) const NESTED_ID_SHIFT: u32 = 32;
const NESTED_ID_MASK: u64 = (1 << NESTED_ID_SHIFT) - 1;

/// The control an id in [`snapshot`] names: one of the surface entered now, or one of a surface nested in its frame, which [`NestedSurface::place`] gave an id the surface's own never take.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccessTarget {
    Own(focus::FocusId),
    Nested { surface: u64, id: focus::FocusId },
}

impl AccessTarget {
    pub fn of(id: u64) -> Self {
        match id >> NESTED_ID_SHIFT {
            0 => Self::Own(id),
            surface => Self::Nested {
                surface,
                id: id & NESTED_ID_MASK,
            },
        }
    }
}

/// Carries out a screen reader's request to move to the control `id` names in [`snapshot`] of the surface entered now, or to activate it.
///
/// A control of its own takes focus, and activation goes out as the Enter it already answers, so a reader pressing a button travels the route a keyboard does. One of a nested surface is reached the way a box event for it is, as an [`Event::AccessAction`] inside [`Event::InSurface`], which the frames along the way pass on to the surface that drew it; one of a [foreign](crate::ForeignSurface) surface, which no frame here can enter, is handed to that surface directly, and its focus stop takes focus once focus is inside. What has to travel through the surface's tree is handed to `dispatch`.
pub fn perform_action(id: u64, activate: bool, dispatch: impl FnOnce(&Event)) {
    match AccessTarget::of(id) {
        AccessTarget::Own(id) => {
            focus::request(id);
            if activate {
                dispatch(&Event::KeyPressed {
                    key: Key::Named(NamedKey::Enter),
                    modifiers: ModifiersState::default(),
                    unmodified: None,
                });
            }
        }
        AccessTarget::Nested { surface, id } => match crate::foreign_surface::target(surface, id) {
            Some((foreign, own)) => {
                foreign.perform_access_action(own, activate);
                crate::foreign_surface::follow_focus_inside(surface);
            }
            None => dispatch(&Event::InSurface {
                surface,
                event: Box::new(Event::AccessAction { id, activate }),
            }),
        },
    }
}

/// The surfaces drawn directly inside `commands`, in draw order. One nested deeper is inside one of these, and is found in its commands.
pub fn nested_surfaces(commands: &[DrawCommand]) -> Vec<NestedSurface> {
    let mut found = Vec::new();
    let mut open: Option<(u64, usize, [f32; 6])> = None;
    let mut depth = 0usize;
    let mut index = 0usize;
    renderer_core::for_each_with_matrix(commands, |command, matrix| {
        let at = index;
        index += 1;
        match command {
            DrawCommand::PushSurface { surface } => {
                if depth == 0 {
                    open = Some((*surface, at + 1, matrix));
                }
                depth += 1;
            }
            DrawCommand::PopSurface if depth > 0 => {
                depth -= 1;
                if depth == 0
                    && let Some((surface, start, placement)) = open.take()
                {
                    found.push(NestedSurface {
                        surface,
                        commands: start..at,
                        placement,
                    });
                }
            }
            _ => {}
        }
    });
    found
}

#[cfg(test)]
#[path = "accessibility_test.rs"]
mod tests;
