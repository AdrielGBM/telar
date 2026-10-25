//! What an application says about a box for assistive technology: the name it goes by, what it is, the language it is in, and whether a reader skips it.
//!
//! Kept beside the tree rather than on each widget, so every widget that owns a layout node takes it the same way and every target reads it from the same place: a document merges it into the element it writes, and the accessibility snapshot resolves it along the frame's element nesting.

use std::sync::Arc;

use layout_core::NodeId;
use reactive_core::{RwSignal, signal};
use renderer_core::{Annotation, Modal, Role};
use rustc_hash::FxHashMap;

use crate::layout_item::LayoutItem;

reactive_core::surface_local! {
    /// Per surface: node ids are minted per surface, so one map would have windows overwrite each other's annotations.
    slot ANNOTATIONS: Annotations = Annotations::default();
    access with_annotations, with_annotations_ref;
    context AnnotationsContext;
}

#[derive(Default)]
struct Annotations {
    by_node: FxHashMap<NodeId, Slot>,
    made: u64,
}

#[derive(Clone, Copy)]
struct Slot {
    annotation: RwSignal<Annotation>,
    /// Which making of the slot this is, so a withdrawal arriving after the node id was reused leaves the new widget's alone.
    generation: u64,
}

/// Forgets every annotation, for a tree being replaced wholesale.
pub(crate) fn reset() {
    with_annotations(|a| a.by_node.clear());
}

fn signal_of(node: NodeId) -> Option<RwSignal<Annotation>> {
    with_annotations_ref(|a| a.by_node.get(&node).map(|slot| slot.annotation))
}

/// What was said about `node`, subscribing the caller to it. `None` for the overwhelming majority of boxes, which say nothing.
pub(crate) fn of(node: NodeId) -> Option<Annotation> {
    signal_of(node)?
        .try_get()
        .filter(|annotation| !annotation.is_empty())
}

/// As [`of`], without subscribing: for a reader walking a finished frame.
pub(crate) fn peek(node: NodeId) -> Option<Annotation> {
    let annotation = signal_of(node).filter(RwSignal::is_alive)?;
    Some(annotation.peek_with(Clone::clone)).filter(|annotation| !annotation.is_empty())
}

/// The language `node` is written in: the nearest `lang` said on it or above it, or the surface's own, the active locale. Resolved along the layout tree, the nesting a document writes its `lang` attributes along, so the case a shaper maps text into follows the language the page declares.
pub(crate) fn language_at(node: NodeId) -> Option<Arc<str>> {
    layout_reactive::ancestors(node)
        .find_map(|at| of(at).and_then(|annotation| annotation.lang))
        .or_else(|| i18n_core::use_locale().map(Arc::from))
}

/// The signal `node`'s annotation lives in, made on first use and withdrawn with the owner that made it.
pub(crate) fn slot(node: NodeId) -> RwSignal<Annotation> {
    if let Some(existing) = signal_of(node)
        && existing.is_alive()
    {
        return existing;
    }
    let annotation = signal(Annotation::default());
    let generation = with_annotations(|a| {
        a.made += 1;
        a.by_node.insert(
            node,
            Slot {
                annotation,
                generation: a.made,
            },
        );
        a.made
    });
    reactive_core::on_cleanup(move || forget(node, generation));
    annotation
}

fn forget(node: NodeId, generation: u64) {
    with_annotations(|a| {
        if a.by_node
            .get(&node)
            .is_some_and(|slot| slot.generation == generation)
        {
            a.by_node.remove(&node);
        }
    });
}

/// Says `node` is one of the boxes the modal layer `layer` is made of. Said before the box is first drawn, so the element it draws subscribes to it.
pub(crate) fn mark_modal(node: NodeId, layer: NodeId) {
    slot(node).update(|annotation| {
        annotation.modal = Some(Modal {
            layer: layer.into(),
            opened_from: None,
        })
    });
}

/// Names the control the modal layer `node` was opened from, once it is known.
pub(crate) fn modal_opened_from(node: NodeId, owner: NodeId) {
    let Some(annotation) = signal_of(node).filter(RwSignal::is_alive) else {
        return;
    };
    annotation.update(|annotation| {
        if let Some(modal) = &mut annotation.modal {
            modal.opened_from = Some(owner.into());
        }
    });
}

/// Keeps one part of `node`'s annotation in step with what `amend` reads.
fn follow(node: NodeId, amend: impl Fn(&mut Annotation) + 'static) {
    let annotation = slot(node);
    reactive_core::effect(move || {
        let mut next = annotation.peek();
        amend(&mut next);
        if annotation.peek() != next {
            annotation.set(next);
        }
    });
}

/// Keeps `node`'s name at what `read` answers, `None` or an empty name being none.
pub(crate) fn label_follows<S: Into<Arc<str>>>(
    node: NodeId,
    read: impl Fn() -> Option<S> + 'static,
) {
    follow(node, move |annotation| {
        annotation.label = read().map(Into::into).filter(|label| !label.is_empty());
    });
}

/// The annotations every widget with a layout node takes. See the module docs.
///
/// Prefixed because a widget may well have a `label` of its own that means something else — the text a button draws — and a method of that name here would silently lose to it.
pub trait Accessible: LayoutItem + Sized {
    /// The name assistive technology reads for this box. Re-read when what `label` reads changes, so a translated name follows the locale.
    ///
    /// A name, not a replacement for the content: a reader that walks into the box still finds what is drawn there, which is what [`a11y_hidden`](Self::a11y_hidden) on the pieces is for.
    ///
    /// An empty name is no name: the box goes by what it draws, as if it had been given none. A control named `""` would otherwise be announced as nothing at all.
    fn a11y_label<S: Into<Arc<str>>>(self, label: impl Fn() -> S + 'static) -> Self {
        label_follows(self.layout_node(), move || Some(label()));
        self
    }

    /// What this box is, said from outside the widget that made it: a region, an article, a heading, a live region, an item of the list around it, or a plain group ([`Role::is_annotatable`]).
    ///
    /// Taken only by a box that is no more than a group or a region and that the keyboard does not reach ([`Role::annotated`]): a control, a list or anything else the widget made it keeps that role on every target, since what a box is decides how it is reached and what pressing it does.
    fn a11y_role(self, role: Role) -> Self {
        debug_assert!(
            role.is_annotatable(),
            "`{}` is not a role that can be said from outside a widget",
            role.as_str()
        );
        follow(self.layout_node(), move |annotation| {
            annotation.role = Some(role);
        });
        self
    }

    /// The language this box and everything under it is written in, as a BCP 47 tag. Nearer boxes win, so a quotation inside a paragraph says only its own.
    fn a11y_lang<S: Into<Arc<str>>>(self, lang: impl Fn() -> S + 'static) -> Self {
        follow(self.layout_node(), move |annotation| {
            annotation.lang = Some(lang().into());
        });
        self
    }

    /// Takes this box and everything under it out of what assistive technology is told. It is still drawn and still answers the pointer.
    fn a11y_hidden(self) -> Self {
        follow(self.layout_node(), |annotation| annotation.hidden = true);
        self
    }
}

impl<T: LayoutItem> Accessible for T {}

#[cfg(test)]
#[path = "annotation_test.rs"]
mod tests;
