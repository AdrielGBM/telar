//! Every widget that owns a layout node has to appear as an element, and the reason is structural rather than cosmetic: on a document target CSS does the layout, so a node with no element is a box the browser never creates — and its children are then laid out by the wrong parent, in the wrong flow, with the wrong gap. One missing element is not one missing box; it is every box under it in the wrong place.

use geometry_core::Rect;
use layout_core::NodeId;
use renderer_core::{Element, ElementId, Semantics};
use std::sync::Arc;

/// What a box is: what it was told, or what it does.
///
/// A box that answers a press *is* a button whether or not anybody said so, and the derivation is what keeps most of an interface meaningful without a word of markup. It stays a derivation and not a default, so a box that says it is a menu item is one, press or no press.
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

/// Built lazily: calling `document` eagerly would subscribe this view to whatever it reads, even when nothing here needs more than the box's identity.
pub(crate) fn for_target(node: NodeId, document: impl FnOnce() -> Arc<Element>) -> Arc<Element> {
    if ui_tree::element_capture() {
        document()
    } else {
        identity(node)
    }
}

pub(crate) fn identity(node: NodeId) -> Arc<Element> {
    Arc::new(Element::new(
        ElementId(node.into()),
        Semantics::group(),
        "",
        Rect::default(),
    ))
}

/// The element for a node that means something more than a box, asking the backend to put its own scroll at `scroll_to`. See [`renderer_core::Element::scroll_to`]; every box but a scroll area that is being moved passes `None`.
pub(crate) fn with_semantics_scrolled(
    node: NodeId,
    semantics: Semantics,
    scroll_to: Option<(f32, f32)>,
) -> Arc<Element> {
    Arc::new(element_of(node, semantics).asking_to_scroll(scroll_to))
}

/// The element for a node that means something more than a box.
pub(crate) fn with_semantics(node: NodeId, semantics: Semantics) -> Arc<Element> {
    Arc::new(element_of(node, semantics))
}

fn element_of(node: NodeId, semantics: Semantics) -> Element {
    let layout = layout_reactive::declared_css(node)
        .map(|css| css.into_string())
        .unwrap_or_default();
    // Read, not peeked: a box that moves has to re-emit, because where it is is part of what a document needs.
    let rect = layout_reactive::track_layout(node)
        .map(|rect| rect.get())
        .unwrap_or_default();
    let semantics = match crate::annotation::of(node) {
        Some(annotation) => semantics.annotated(&annotation),
        None => semantics,
    };
    Element::new(ElementId(node.into()), semantics, layout, rect)
}

/// The shape most widgets need: they own a node, they draw children into it, and they have nothing to say about what it means beyond being a box.
pub(crate) fn wrap(node: NodeId, content: ui_tree::RenderNode) -> ui_tree::RenderNode {
    let element = for_target(node, || with_semantics(node, Semantics::group()));
    ui_tree::RenderNode::element(element, [content])
}
