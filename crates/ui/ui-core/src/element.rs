//! Every widget that owns a layout node has to appear as an element, and the reason is structural rather than cosmetic: on a document target CSS does the layout, so a node with no element is a box the browser never creates — and its children are then laid out by the wrong parent, in the wrong flow, with the wrong gap. One missing element is not one missing box; it is every box under it in the wrong place.

use geometry_core::Rect;
use layout_core::NodeId;
use renderer_core::{Editing, Element, ElementId, Semantics};
use std::sync::Arc;

/// What a box is: what it was told, or what it does.
///
/// A box a primary press or the keyboard activates *is* a button whether or not anybody said so; one answering only an alt or a long press is not, or a reader would offer an activation that does nothing over the controls it wraps. The derivation is what keeps most of an interface meaningful without a word of markup. It stays a derivation and not a default, so a box that says it is a menu item is one, press or no press.
pub(crate) fn role_of(
    declared: Option<renderer_core::Role>,
    activates: bool,
) -> renderer_core::Role {
    match (declared, activates) {
        (Some(role), _) => role,
        (None, true) => renderer_core::Role::Button,
        (None, false) => renderer_core::Role::Group,
    }
}

/// Built lazily: calling `document` eagerly would subscribe this view to whatever it reads, even when nothing here needs more than the box's identity and `role`.
///
/// The role travels on every target because the accessibility snapshot reads it along the frame's element nesting: a named tab list names itself, where a named box with no role names the one control it wraps.
pub(crate) fn for_target(
    node: NodeId,
    role: renderer_core::Role,
    document: impl FnOnce() -> Arc<Element>,
) -> Arc<Element> {
    for_stated_target(node, role, |semantics| semantics, document)
}

/// [`for_target`] for a box that is no control and says something of itself a reader cannot see, such as where it sits among the items of its set or how far a bar has filled: what `state` adds to the role travels with it, since the snapshot reads it along the same nesting and a box that only says so is announced for it. `state` is read only where the document is not built, which reads it itself.
pub(crate) fn for_stated_target(
    node: NodeId,
    role: renderer_core::Role,
    state: impl FnOnce(Semantics) -> Semantics,
    document: impl FnOnce() -> Arc<Element>,
) -> Arc<Element> {
    if ui_tree::element_capture() {
        document()
    } else {
        identity_as(node, state(Semantics::of(role)))
    }
}

pub(crate) fn identity(node: NodeId) -> Arc<Element> {
    identity_as(node, Semantics::of(renderer_core::Role::Group))
}

fn identity_as(node: NodeId, semantics: Semantics) -> Arc<Element> {
    Arc::new(Element::new(
        ElementId(node.into()),
        semantics,
        "",
        Rect::default(),
    ))
}

/// [`identity`], for a box that is a link to `destination`, and the current one of its set when `current` says so.
pub(crate) fn identity_linking(
    node: NodeId,
    destination: platform_core::Destination,
    current: bool,
) -> Arc<Element> {
    Arc::new(Element::new(
        ElementId(node.into()),
        Semantics::group()
            .linking_to(destination)
            .marked_current(current),
        "",
        Rect::default(),
    ))
}

/// The element for a box that scrolls, asking the backend to put its own scroll at `scroll_to`, saying whether it is the surface's primary scroll and how far short of its edges it stops what it brings into view. See [`renderer_core::Element::scroll_to`], [`renderer_core::Element::primary_scroll`] and [`renderer_core::Element::arrival_margin`].
pub(crate) fn with_semantics_scrolled(
    node: NodeId,
    semantics: Semantics,
    scroll_to: Option<(f32, f32)>,
    primary: bool,
    arrival_margin: geometry_core::Insets,
) -> Arc<Element> {
    Arc::new(
        element_of(node, semantics)
            .scrolling()
            .asking_to_scroll(scroll_to)
            .as_primary_scroll(primary)
            .arriving_within(arrival_margin),
    )
}

/// The element for the box a layer fixed over the surface is made of, declared where `place` stands. See [`renderer_core::Element::fixed_in_place_of`].
pub(crate) fn fixed_in_place_of(node: NodeId, place: NodeId) -> Arc<Element> {
    Arc::new(element_of(node, Semantics::group()).fixed_in_place_of(ElementId(place.into())))
}

/// The element for a node that means something more than a box.
pub(crate) fn with_semantics(node: NodeId, semantics: Semantics) -> Arc<Element> {
    Arc::new(element_of(node, semantics))
}

/// The element for a field, carrying what it shows while it holds the keyboard. See [`renderer_core::Element::editing`].
pub(crate) fn editing(node: NodeId, semantics: Semantics, editing: Editing) -> Arc<Element> {
    Arc::new(element_of(node, semantics).editing(Some(editing)))
}

pub(crate) fn showing(
    node: NodeId,
    semantics: Semantics,
    picture: renderer_core::Picture,
) -> Arc<Element> {
    Arc::new(element_of(node, semantics).showing(picture))
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
    let element = for_target(node, renderer_core::Role::Group, || {
        with_semantics(node, Semantics::group())
    });
    ui_tree::RenderNode::element(element, [content])
}
