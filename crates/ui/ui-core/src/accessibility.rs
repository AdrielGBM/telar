//! What a screen reader is told about the window right now.
//!
//! Telar had the whole of *operating* an interface without a mouse — a tab order scoped to what is genuinely reachable, arrow keys and type-ahead in every list, a focus ring that knows a tap from a Tab — and none of it reaches someone who cannot see the screen. That is a different question: not *where do the keys go* but *what is this and what state is it in*.
//!
//! Almost nothing here is authored per widget, and that is the design. The set of controls is the focus registry's own tab order, so the reader and the keyboard cannot come to different conclusions about what is on screen. The name is taken from **the text the widget actually draws inside itself**, which is how it reads to everyone else and needs no second copy that can fall out of step with the first. What an application does author — a name, a language, a box a reader skips — is resolved along the frame's own element nesting, the same nesting a document backend turns into elements.

use std::sync::Arc;

use geometry_core::Rect;
use layout_core::NodeId;
use platform_core::{AccessNode, Role};
use renderer_core::DrawCommand;
use rustc_hash::FxHashMap;

use crate::focus;

/// Whether the snapshot reads `command`, for a runner that keeps the last frame's reading between frames rather than the whole frame.
pub fn is_read(command: &DrawCommand) -> bool {
    matches!(
        command,
        DrawCommand::Text { .. }
            | DrawCommand::Image { .. }
            | DrawCommand::Path { .. }
            | DrawCommand::PushElement { .. }
            | DrawCommand::PopElement
    )
}

/// Everything the platform's accessibility layer needs to describe the window, in reading order.
///
/// `commands` is the frame the renderer is about to draw — the same list, so what is announced and what is painted are the same picture by construction rather than by agreement.
pub fn snapshot(commands: &[DrawCommand]) -> Vec<AccessNode> {
    let reading = Reading::of(commands);
    let controls: Vec<(focus::Exposed, Rect)> = focus::exposed()
        .into_iter()
        .filter(|e| !reading.scope(e.node).hidden)
        .filter_map(|e| layout_reactive::absolute_rect(e.node).map(|rect| (e, rect)))
        .collect();
    let focused = focus::current();

    let mut named_controls = vec![false; controls.len()];
    let mut nodes: Vec<AccessNode> = controls
        .iter()
        .enumerate()
        .map(|(i, (e, rect))| {
            let label = reading.label_of(e.node);
            named_controls[i] = label.is_some();
            AccessNode {
                id: Some(e.id),
                role: e.role,
                name: label.map(str::to_string).unwrap_or_default(),
                rect: *rect,
                focused: focused == Some(e.id),
                enabled: e.enabled,
                toggled: e.toggled,
                value: e.value,
                lang: reading.scope(e.node).lang.as_deref().map(str::to_string),
            }
        })
        .collect();

    let is_control = |node: NodeId| controls.iter().any(|(e, _)| e.node == node);
    let pieces = reading
        .named
        .iter()
        .filter(|named| !is_control(named.node))
        .filter_map(|named| {
            let rect = layout_reactive::absolute_rect(named.node)?;
            // A named box that draws artwork and no words is a picture; anything else reads as the text it stands for.
            let role = if named.drew_art && !named.drew_text {
                Role::Drawing
            } else {
                Role::Label
            };
            Some(Piece {
                text: named.label.to_string(),
                rect,
                role,
                lang: named.lang.clone(),
            })
        })
        .chain(reading.text.iter().cloned());

    for piece in pieces {
        // The smallest control containing it, so a button inside a card is named by its own label.
        let owner = controls
            .iter()
            .enumerate()
            .filter(|(_, (_, bounds))| contains(*bounds, piece.rect))
            .min_by(|(_, (_, a)), (_, (_, b))| area(*a).total_cmp(&area(*b)));
        match owner {
            // A control the application named is called that, whatever it draws.
            Some((i, _)) if named_controls[i] => {}
            Some((i, _)) => append(&mut nodes[i].name, &piece.text),
            // Text belonging to no control is still content: a heading, a caption, the paragraph a dialog asks about.
            None => nodes.push(AccessNode {
                id: None,
                role: piece.role,
                name: piece.text,
                rect: piece.rect,
                focused: false,
                enabled: true,
                toggled: None,
                value: None,
                lang: piece.lang.as_deref().map(str::to_string),
            }),
        }
    }

    // Reading order rather than tab order: a label sits between the controls it explains. Tab order stays the focus registry's answer.
    nodes.sort_by(|a, b| {
        a.rect
            .y
            .total_cmp(&b.rect.y)
            .then(a.rect.x.total_cmp(&b.rect.x))
    });
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
    drew_text: bool,
    drew_art: bool,
}

/// One frame, as a reader meets it: the annotations resolved along its element nesting, with everything under a hidden box already gone.
#[derive(Default)]
struct Reading {
    scopes: FxHashMap<NodeId, Scope>,
    named: Vec<Named>,
    text: Vec<Piece>,
}

impl Reading {
    fn of(commands: &[DrawCommand]) -> Self {
        let mut reading = Self::default();
        // The scope in force and the innermost named box, per open element.
        let mut open: Vec<(Scope, Option<usize>)> = Vec::new();
        for command in commands {
            let (scope, named) = open.last().cloned().unwrap_or_default();
            match command {
                DrawCommand::PushElement { element } => {
                    let node = NodeId::from(element.id.0);
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
                                drew_text: false,
                                drew_art: false,
                            });
                            Some(reading.named.len() - 1)
                        }
                        _ => named,
                    };
                    reading.scopes.insert(node, inner.clone());
                    open.push((inner, named));
                }
                DrawCommand::PopElement => {
                    open.pop();
                }
                _ if scope.hidden => {}
                DrawCommand::Text { text, rect, .. } if !text.trim().is_empty() => {
                    if let Some(i) = named {
                        reading.named[i].drew_text = true;
                    }
                    reading.text.push(Piece {
                        text: text.to_string(),
                        rect: *rect,
                        role: Role::Label,
                        lang: scope.lang,
                    });
                }
                DrawCommand::Image { .. } | DrawCommand::Path { .. } => {
                    if let Some(i) = named {
                        reading.named[i].drew_art = true;
                    }
                }
                _ => {}
            }
        }
        reading
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

#[cfg(test)]
#[path = "accessibility_test.rs"]
mod tests;
