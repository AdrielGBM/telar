//! What a screen reader is told about the window right now.
//!
//! Telar had the whole of *operating* an interface without a mouse — a tab order scoped to what is genuinely reachable, arrow keys and type-ahead in every list, a focus ring that knows a tap from a Tab — and none of it reaches someone who cannot see the screen. That is a different question: not *where do the keys go* but *what is this and what state is it in*.
//!
//! Nothing here is authored per widget, and that is the design. The set of controls is the focus registry's own tab order, so the reader and the keyboard cannot come to different conclusions about what is on screen. The name is taken from **the text the widget actually draws inside itself**, which is how it reads to everyone else and needs no second copy that can fall out of step with the first. Only the role is declared, and only where the default of "a thing you activate" is wrong.

use geometry_core::Rect;
use platform_core::{AccessNode, Role};
use renderer_core::DrawCommand;

use crate::focus;

/// Everything the platform's accessibility layer needs to describe the window, in reading order.
///
/// `commands` is the frame the renderer is about to draw — the same list, so what is announced and what is painted are the same picture by construction rather than by agreement.
pub fn snapshot(commands: &[DrawCommand]) -> Vec<AccessNode> {
    let controls: Vec<(focus::Exposed, Rect)> = focus::exposed()
        .into_iter()
        .filter_map(|e| layout_reactive::absolute_rect(e.node).map(|rect| (e, rect)))
        .collect();
    let focused = focus::current();

    let mut nodes: Vec<AccessNode> = controls
        .iter()
        .map(|(e, rect)| AccessNode {
            id: Some(e.id),
            role: e.role,
            name: String::new(),
            rect: *rect,
            focused: focused == Some(e.id),
            enabled: e.enabled,
            toggled: e.toggled,
            value: e.value,
        })
        .collect();

    for (text, rect) in drawn_text(commands) {
        // The smallest control containing it, so a button inside a card is named by its own label.
        let owner = controls
            .iter()
            .enumerate()
            .filter(|(_, (_, bounds))| contains(*bounds, rect))
            .min_by(|(_, (_, a)), (_, (_, b))| area(*a).total_cmp(&area(*b)));
        match owner {
            Some((i, _)) => append(&mut nodes[i].name, &text),
            // Text belonging to no control is still content: a heading, a caption, the paragraph a dialog asks about.
            None => nodes.push(AccessNode {
                id: None,
                role: Role::Label,
                name: text,
                rect,
                focused: false,
                enabled: true,
                toggled: None,
                value: None,
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

/// Every string the frame draws, with where it draws it.
fn drawn_text(commands: &[DrawCommand]) -> Vec<(String, Rect)> {
    commands
        .iter()
        .filter_map(|command| match command {
            DrawCommand::Text { text, rect, .. } => Some((text.to_string(), *rect)),
            _ => None,
        })
        .filter(|(text, _)| !text.trim().is_empty())
        .collect()
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
