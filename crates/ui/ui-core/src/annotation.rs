//! What an application says about a box for assistive technology: the name it goes by, the language it is in, and whether a reader skips it.
//!
//! Kept beside the tree rather than on each widget, so every widget that owns a layout node takes it the same way and every target reads it from the same place: a document merges it into the element it writes, and the accessibility snapshot resolves it along the frame's element nesting.

use std::sync::Arc;

use layout_core::NodeId;
use reactive_core::{RwSignal, signal};
use renderer_core::Annotation;
use rustc_hash::FxHashMap;

use crate::layout_item::LayoutItem;

reactive_core::surface_local! {
    /// Per surface: node ids are minted per surface, so one map would have windows overwrite each other's annotations.
    slot ANNOTATIONS: Annotations = Annotations::default();
    access with_annotations, with_annotations_ref;
    context AnnotationsContext, AnnotationsGuard;
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

/// The signal `node`'s annotation lives in, made on first use and withdrawn with the owner that made it.
fn slot(node: NodeId) -> RwSignal<Annotation> {
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

/// The annotations every widget with a layout node takes. See the module docs.
///
/// Prefixed because a widget may well have a `label` of its own that means something else — the text a button draws — and a method of that name here would silently lose to it.
pub trait Accessible: LayoutItem + Sized {
    /// The name assistive technology reads for this box. Re-read when what `label` reads changes, so a translated name follows the locale.
    ///
    /// A name, not a replacement for the content: a reader that walks into the box still finds what is drawn there, which is what [`a11y_hidden`](Self::a11y_hidden) on the pieces is for.
    fn a11y_label<S: Into<Arc<str>>>(self, label: impl Fn() -> S + 'static) -> Self {
        follow(self.layout_node(), move |annotation| {
            annotation.label = Some(label().into());
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
