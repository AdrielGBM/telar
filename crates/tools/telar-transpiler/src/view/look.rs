//! `look:` on a box: the value a box is painted from, through `telar::Look`, written out before the view is generated as the paint attributes it stands for, so the box is built, and its captures counted, exactly as if the author had written them.
//!
//! What the box writes itself wins part by part, as it wins over a class, and the look wins over the classes the box names: a paint attribute the box writes replaces that part of the look, a `hover_style`/`active_style` it writes takes the look's fill for that state unless it names a fill of its own, and the pointer states are painted only on a box that answers the pointer anyway, so a look never makes a box a pointer target.

use telar_parser::{Attr, Element, Value, ViewNode};

use crate::registry::LOOK_ATTR;

/// The attribute each part of a look is written as, and the `telar::Look` method that paints it.
const PARTS: &[(&str, &str)] = &[("fill", "fill"), ("stroke", "frame"), ("color", "ink")];

/// The state style each pointer state of a look is written into, and the method that paints its fill.
const STATES: &[(&str, &str)] = &[("hover_style", "hovered"), ("active_style", "pressed")];

/// `nodes` with every box's look written out, or `None` when no box in them has one.
pub(super) fn written_out(nodes: &[ViewNode]) -> Option<Vec<ViewNode>> {
    nodes
        .iter()
        .any(holds_look)
        .then(|| nodes.iter().map(write_out).collect())
}

/// Why `el`'s `look` paints nothing, for one written as a bare flag.
pub(super) fn flag_error(el: &Element) -> Option<String> {
    el.attributes
        .iter()
        .any(|a| a.key == LOOK_ATTR && a.value.is_flag())
        .then(|| {
            "`look:` names the value the box is painted from, one implementing `telar::Look`: `look:(value)`".to_string()
        })
}

fn is_box(tag: &str) -> bool {
    matches!(tag, "row" | "col" | "grid" | "box")
}

fn holds_look(node: &ViewNode) -> bool {
    match node {
        ViewNode::Element(el) => {
            (is_box(&el.tag) && el.attributes.iter().any(|a| a.key == LOOK_ATTR))
                || el.children.iter().any(holds_look)
        }
        ViewNode::IfBlock(block) => block
            .then_branch
            .iter()
            .chain(block.else_branch.iter().flatten())
            .any(holds_look),
        ViewNode::ForBlock(block) => block.body.iter().any(holds_look),
        ViewNode::MatchBlock(block) => block.arms.iter().flat_map(|arm| &arm.body).any(holds_look),
        ViewNode::LetStmt(_) | ViewNode::Comment(_) => false,
    }
}

fn write_out(node: &ViewNode) -> ViewNode {
    let all = |nodes: &[ViewNode]| nodes.iter().map(write_out).collect::<Vec<_>>();
    match node {
        ViewNode::Element(el) => {
            let mut el = Element {
                children: all(&el.children),
                ..el.clone()
            };
            if is_box(&el.tag) {
                write_look(&mut el);
            }
            ViewNode::Element(el)
        }
        ViewNode::IfBlock(block) => {
            let mut block = block.clone();
            block.then_branch = all(&block.then_branch);
            block.else_branch = block.else_branch.as_deref().map(all);
            ViewNode::IfBlock(block)
        }
        ViewNode::ForBlock(block) => {
            let mut block = block.clone();
            block.body = all(&block.body);
            ViewNode::ForBlock(block)
        }
        ViewNode::MatchBlock(block) => {
            let mut block = block.clone();
            for arm in &mut block.arms {
                arm.body = all(&arm.body);
            }
            ViewNode::MatchBlock(block)
        }
        ViewNode::LetStmt(_) | ViewNode::Comment(_) => node.clone(),
    }
}

/// Appends the attributes `el`'s look stands for, each read in the ink of the text around the box. The `look` attribute stays: no emitter builds anything from it.
fn write_look(el: &mut Element) {
    let Some(look) = el
        .attributes
        .iter()
        .find(|a| a.key == LOOK_ATTR && !a.value.is_flag())
        .cloned()
    else {
        return;
    };
    let value = look.value.text().trim();
    let value = super::redundant_parens(value).unwrap_or(value);
    let painted = |method: &str| format!("(::telar::Look::{method}(&({value}), $text.ink))");
    let written = |key: &str, value: Value| Attr {
        key: key.to_string(),
        value,
        value_start: look.value_start,
    };
    for (key, method) in PARTS {
        if !el.attributes.iter().any(|a| a.key == *key) {
            el.attributes
                .push(written(key, Value::Expr(painted(method))));
        }
    }
    if !super::container::answers_pointer(el) {
        return;
    }
    for (key, method) in STATES {
        let fill = format!("fill:{}", painted(method));
        match el.attributes.iter_mut().find(|a| a.key == *key) {
            Some(own) => {
                let names_fill = super::container::parse_inline_paint_attrs(own.value.text(), 0)
                    .iter()
                    .any(|a| a.key == "fill");
                if !names_fill {
                    own.value = Value::Directive(format!("{} {fill}", own.value.text()));
                }
            }
            None => el.attributes.push(written(key, Value::Directive(fill))),
        }
    }
}

#[cfg(test)]
#[path = "look.test.rs"]
mod tests;
