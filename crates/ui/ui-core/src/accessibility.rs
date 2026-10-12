//! What a screen reader is told about the window right now.
//!
//! Telar had the whole of *operating* an interface without a mouse — a tab order scoped to what is genuinely reachable, arrow keys and type-ahead in every list, a focus ring that knows a tap from a Tab — and none of it reaches someone who cannot see the screen. That is a different question: not *where do the keys go* but *what is this and what state is it in*.
//!
//! Almost nothing here is authored per widget, and that is the design. The set of controls is the focus registry's own tab order, so the reader and the keyboard cannot come to different conclusions about what is on screen. The name is taken from **the text the widget actually draws inside itself**, which is how it reads to everyone else and needs no second copy that can fall out of step with the first. What an application does author — a name, a language, a box a reader skips — is resolved along the frame's own element nesting, the same nesting a document backend turns into elements.

use std::ops::Range;
use std::sync::Arc;

use geometry_core::Rect;
use layout_core::NodeId;
use platform_core::{AccessNode, CurrentKind, Event, Key, ModifiersState, NamedKey, Role};
use renderer_core::DrawCommand;
use rustc_hash::{FxHashMap, FxHashSet};

use reactive_core::SurfaceHandle;

use crate::focus;
use crate::input_region::{hit_rect, visible_rect};
use crate::surface_context::{Surface, SurfaceGuard};

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

/// Everything the platform's accessibility layer needs to describe the window, in reading order.
///
/// `commands` is the frame the renderer is about to draw — the same list, so what is announced and what is painted are the same picture by construction rather than by agreement.
///
/// A surface drawn inside this one's frame ([`DrawCommand::PushSurface`]) is read in its own world, where its boxes' ids mean its boxes, and its nodes are placed where the frame shows them: see [`NestedSurface::place`].
pub fn snapshot(commands: &[DrawCommand]) -> Vec<AccessNode> {
    let mut nodes = surface_snapshot(commands);
    for nested in nested_surfaces(commands) {
        let Some(_entered) = nested.enter() else {
            continue;
        };
        nodes.extend(
            snapshot(&commands[nested.commands.clone()])
                .into_iter()
                .map(|node| nested.place(node)),
        );
    }
    sort_in_reading_order(&mut nodes);
    nodes
}

/// What a reader is told about the surface entered now, which drew `commands`, leaving out every surface nested in its frame: [`snapshot`] for one surface, as a check that reads each surface in its own world wants it.
pub fn surface_snapshot(commands: &[DrawCommand]) -> Vec<AccessNode> {
    let exposed = focus::exposed();
    let control_nodes: FxHashSet<NodeId> = exposed.iter().map(|e| e.node).collect();
    let reading = Reading::of(commands, &control_nodes);
    let controls: Vec<(focus::Exposed, Rect)> = exposed
        .into_iter()
        .filter(|e| !reading.scope(e.node).hidden)
        .filter_map(|e| visible_rect(e.node).map(|rect| (e, rect)))
        .collect();
    let focused = focus::current();

    let mut named_controls = vec![false; controls.len()];
    let mut nodes: Vec<AccessNode> = controls
        .iter()
        .enumerate()
        .map(|(i, (e, rect))| {
            let label = reading.label_of(e.node);
            named_controls[i] = label.is_some();
            let link = reading.links.get(&e.node);
            AccessNode {
                id: Some(e.id),
                role: e.role,
                name: label.map(str::to_string).unwrap_or_default(),
                rect: hit_rect(e.node).unwrap_or(*rect),
                focused: focused == Some(e.id),
                enabled: e.enabled,
                toggled: e.toggled,
                value: e.value,
                orientation: e.orientation,
                expanded: e.expanded,
                position: e.position,
                lang: reading.scope(e.node).lang.as_deref().map(str::to_string),
                url: link.map(|(destination, _)| platform_core::address_of(destination)),
                current: link.and_then(|(_, current)| *current),
                active_descendant: focus::active_descendant_state(e.id)
                    .filter(|item| controls.iter().any(|(c, _)| c.id == *item)),
            }
        })
        .collect();

    let is_control = |node: NodeId| controls.iter().any(|(e, _)| e.node == node);
    let pieces = reading
        .named
        .iter()
        .filter(|named| !is_control(named.node))
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
                named_box: Some(NamedBox {
                    node: named.node,
                    wraps: wrapped_control(named, &controls),
                }),
                drawn_at: Some(named.opened_at),
            })
        })
        .chain(reading.text.iter().cloned());

    for piece in pieces {
        let owner = match &piece.named_box {
            Some(NamedBox { wraps: Some(i), .. }) => Some(*i),
            // The smallest control containing it, so a button inside a card is named by its own label.
            _ => controls
                .iter()
                .enumerate()
                .filter(|(_, (_, bounds))| contains(*bounds, piece.rect))
                .filter(|(_, (control, _))| !piece.groups(control.node))
                .filter(|(_, (control, _))| !piece.lies_under(control.node, &reading))
                .min_by(|(_, (_, a)), (_, (_, b))| area(*a).total_cmp(&area(*b)))
                .map(|(i, _)| i),
        };
        match owner {
            // A control the application named is called that, whatever it draws.
            Some(i) if named_controls[i] && piece.url.is_none() => {}
            Some(i) if piece.url.is_none() => append(&mut nodes[i].name, &piece.text),
            // Text belonging to no control is still content: a heading, a caption, the paragraph a dialog asks about. A link run is its own node wherever it sits.
            _ => nodes.push(AccessNode {
                id: None,
                role: piece.role,
                name: piece.text,
                rect: piece.rect,
                focused: false,
                enabled: true,
                toggled: None,
                value: None,
                orientation: None,
                expanded: None,
                position: None,
                lang: piece.lang.as_deref().map(str::to_string),
                url: piece.url,
                current: None,
                active_descendant: None,
            }),
        }
    }

    sort_in_reading_order(&mut nodes);
    nodes.retain(|n| n.id.is_some() || !n.name.is_empty());
    nodes
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
    /// The box the application named, for a piece that is one rather than a run of text.
    named_box: Option<NamedBox>,
    /// Where in the frame a run of text was drawn, or a named box began.
    drawn_at: Option<usize>,
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
    /// The named box this one sits in, by its place in [`Reading::named`].
    enclosing: Option<usize>,
    /// Where in the frame the box began.
    opened_at: usize,
    drew_text: bool,
    drew_art: bool,
    /// Whether it drew words or artwork outside every control inside it.
    drew_beside_controls: bool,
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
    in_control: bool,
    node: Option<NodeId>,
}

/// One frame, as a reader meets it: the annotations resolved along its element nesting, with everything under a hidden box already gone.
#[derive(Default)]
struct Reading {
    scopes: FxHashMap<NodeId, Scope>,
    named: Vec<Named>,
    text: Vec<Piece>,
    artwork: Vec<SeenArt>,
    /// Where in the frame each control's box began.
    controls_opened: FxHashMap<NodeId, usize>,
    /// Where each link box goes, and what it is the current one of when it is marked so.
    links: FxHashMap<NodeId, (platform_core::Destination, Option<CurrentKind>)>,
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
                in_control,
                node: innermost,
            } = open.last().cloned().unwrap_or_default();
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
                    let named = match annotation.label {
                        Some(label) if !inner.hidden => {
                            reading.named.push(Named {
                                node,
                                label,
                                lang: inner.lang.clone(),
                                role: element.semantics.role,
                                enclosing: named,
                                opened_at: at,
                                drew_text: false,
                                drew_art: false,
                                drew_beside_controls: false,
                            });
                            Some(reading.named.len() - 1)
                        }
                        _ => named,
                    };
                    reading.scopes.insert(node, inner.clone());
                    open.push(Open {
                        scope: inner,
                        named,
                        in_control: in_control || controls.contains(&node),
                        node: Some(node),
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
                    if clipped_away.last().copied().unwrap_or(false) {
                        return;
                    }
                    reading.text.push(Piece {
                        text: text.to_string(),
                        rect: placed,
                        role: Role::Label,
                        lang: scope.lang.clone(),
                        url: None,
                        named_box: None,
                        drawn_at: Some(at),
                    });
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
                            named_box: None,
                            drawn_at: Some(at),
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

/// Reading order rather than tab order: a label sits between the controls it explains. Tab order stays the focus registry's answer.
pub fn sort_in_reading_order(nodes: &mut [AccessNode]) {
    nodes.sort_by(|a, b| {
        a.rect
            .y
            .total_cmp(&b.rect.y)
            .then(a.rect.x.total_cmp(&b.rect.x))
    });
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
    /// Activates the surface's world for as long as the guard lives, as reading what it drew needs; `None` once the surface is gone, when its boxes mean nothing on this thread.
    pub fn enter(&self) -> Option<SurfaceGuard> {
        Surface::find(SurfaceHandle(self.surface)).map(|surface| surface.enter())
    }

    /// Where the frame shows `rect`, given in the surface's coordinates.
    pub fn place_rect(&self, rect: Rect) -> Rect {
        renderer_core::transform_clip_rect(self.placement, rect)
    }

    /// A node read inside the surface, as the frame around it shows it: at its place in the frame, with its id and the one it names as its active descendant made distinct from every id of the frame's own.
    pub fn place(&self, node: AccessNode) -> AccessNode {
        AccessNode {
            rect: self.place_rect(node.rect),
            id: node.id.map(|id| self.qualify(id)),
            active_descendant: node.active_descendant.map(|id| self.qualify(id)),
            ..node
        }
    }

    // Every surface counts its focus ids from the same start, so a nested one's are moved above the frame's; ids already moved by a surface nested deeper are unique as they are, since handles are.
    fn qualify(&self, id: u64) -> u64 {
        if id >> NESTED_ID_SHIFT == 0 {
            (self.surface << NESTED_ID_SHIFT) | id
        } else {
            id
        }
    }
}

const NESTED_ID_SHIFT: u32 = 32;

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
                id: id & ((1 << NESTED_ID_SHIFT) - 1),
            },
        }
    }
}

/// Carries out a screen reader's request to move to the control `id` names in [`snapshot`] of the surface entered now, or to activate it.
///
/// A control of its own takes focus, and activation goes out as the Enter it already answers, so a reader pressing a button travels the route a keyboard does. One of a nested surface is reached the way a box event for it is, as an [`Event::AccessAction`] inside [`Event::InSurface`], which the frames along the way pass on to the surface that drew it. What has to travel through the surface's tree is handed to `dispatch`.
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
        AccessTarget::Nested { surface, id } => dispatch(&Event::InSurface {
            surface,
            event: Box::new(Event::AccessAction { id, activate }),
        }),
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
