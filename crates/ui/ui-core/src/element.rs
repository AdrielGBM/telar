//! Naming a box for a backend whose output is a document.
//!
//! Every widget that owns a layout node has to appear as an element, and the reason is structural rather
//! than cosmetic: on that target CSS does the layout, so a node with no element is a box the browser never
//! creates — and its children are then laid out by the wrong parent, in the wrong flow, with the wrong gap.
//! One missing element is not one missing box; it is every box under it in the wrong place.

use layout_core::NodeId;
use renderer_core::{Element, ElementId, Semantics};
use std::sync::Arc;

/// The element for a node that is only a box.
pub(crate) fn of(node: NodeId) -> Arc<Element> {
    with_semantics(node, Semantics::group())
}

/// What a box is: what it was told, or what it does.
///
/// A box that answers a press *is* a button whether or not anybody said so, and the derivation is what keeps
/// most of an interface meaningful without a word of markup. It stays a derivation and not a default, so a
/// box that says it is a menu item is one, press or no press.
pub(crate) fn role_of(
    declared: Option<renderer_core::Role>,
    pressable: bool,
) -> renderer_core::Role {
    match (declared, pressable) {
        (Some(role), _) => role,
        (None, true) => renderer_core::Role::Button,
        (None, false) => renderer_core::Role::Group,
    }
}

/// The element for a node that means something more than a box.
pub(crate) fn with_semantics(node: NodeId, semantics: Semantics) -> Arc<Element> {
    let layout = layout_reactive::declared_css(node)
        .map(|css| css.into_string())
        .unwrap_or_default();
    // Read, not peeked: a box that moves has to re-emit, because where it is is part of what a document
    // needs for the boxes nothing else places.
    let rect = layout_reactive::track_layout(node)
        .map(|rect| rect.get())
        .unwrap_or_default();
    Arc::new(Element::new(
        ElementId(node.into()),
        semantics,
        layout,
        rect,
    ))
}

/// Wraps `content` as the box `node` names, when a document backend is listening.
///
/// The shape most widgets need: they own a node, they draw children into it, and they have nothing to say
/// about what it means beyond being a box.
pub(crate) fn wrap(node: NodeId, content: ui_tree::RenderNode) -> ui_tree::RenderNode {
    if ui_tree::element_capture() {
        ui_tree::RenderNode::element(of(node), [content])
    } else {
        content
    }
}
