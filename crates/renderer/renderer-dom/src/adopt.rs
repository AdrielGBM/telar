//! Taking over the page a host was served with, instead of building it again.
//!
//! A prerendered page is the document the reconcile would build (`html.rs`), so the first frame finds every box it needs already there, named by `data-telar-id`. It keeps each one whose tag and parent still agree, patches what differs in place, and rebuilds only the subtree under a box that no longer fits. What it never claims is swept once the frame is done, not while a later box may still claim it.

use std::collections::VecDeque;

use rustc_hash::FxHashMap;
use wasm_bindgen::JsCast;

use crate::document::ID_ATTRIBUTE;

pub(crate) struct Served {
    boxes: FxHashMap<u64, web_sys::Element>,
    root_paint: VecDeque<web_sys::Element>,
    /// Each element the frame filled, with how many children it placed: whatever is past them was served and not claimed.
    sweeps: Vec<(web_sys::Node, u32)>,
}

impl Served {
    pub(crate) fn index(host: &web_sys::HtmlElement) -> Self {
        let mut boxes = FxHashMap::default();
        for element in elements(host.query_selector_all(&format!("[{ID_ATTRIBUTE}]")).ok()) {
            if let Some(id) = id_of(&element) {
                boxes.entry(id).or_insert(element);
            }
        }
        Self {
            boxes,
            root_paint: paint_children(host.as_ref()),
            sweeps: Vec::new(),
        }
    }

    /// The served element for box `id`, when it is the element the frame would put under `parent` with `tag`.
    ///
    /// One whose tag or parent differs is no longer that box: everything under it is forgotten as well, so the subtree is rebuilt from the frame as a whole and the stale one is swept with its parent's leftovers.
    pub(crate) fn claim(
        &mut self,
        id: u64,
        tag: &str,
        parent: &web_sys::Node,
    ) -> Option<web_sys::Element> {
        let element = self.boxes.remove(&id)?;
        let found = element.local_name();
        let same_parent = element
            .parent_node()
            .is_some_and(|served| served.is_same_node(Some(parent)));
        if found == tag && same_parent {
            return Some(element);
        }
        for inside in elements(
            element
                .query_selector_all(&format!("[{ID_ATTRIBUTE}]"))
                .ok(),
        ) {
            if let Some(id) = id_of(&inside) {
                self.boxes.remove(&id);
            }
        }
        let reason = if found == tag {
            "it moved to another parent".to_string()
        } else {
            format!("it was served as <{found}>")
        };
        if cfg!(debug_assertions) {
            tracing::warn!(
                "rebuilding the served box {id} as <{tag}>, because {reason}: the page was prerendered from inputs the client does not share"
            );
        } else {
            tracing::debug!("rebuilding the served box {id} as <{tag}>, because {reason}");
        }
        None
    }

    pub(crate) fn next_root_paint(&mut self) -> Option<web_sys::Element> {
        self.root_paint.pop_front()
    }

    pub(crate) fn sweep_past(&mut self, parent: web_sys::Node, placed: u32) {
        self.sweeps.push((parent, placed));
    }

    pub(crate) fn finish(self) {
        for (parent, placed) in self.sweeps {
            crate::reconcile::truncate(&parent, placed);
        }
    }
}

/// The children of `parent` that stand for paint rather than boxes: every element that names no box, in order. The editable element the host keeps for typing is neither.
pub(crate) fn paint_children(parent: &web_sys::Element) -> VecDeque<web_sys::Element> {
    let children = parent.children();
    (0..children.length())
        .filter_map(|index| children.item(index))
        .filter(|child| {
            child.local_name() == "div"
                && !child.has_attribute(ID_ATTRIBUTE)
                && !child.has_attribute(crate::entry::ENTRY_ATTRIBUTE)
        })
        .collect()
}

fn elements(list: Option<web_sys::NodeList>) -> impl Iterator<Item = web_sys::Element> {
    let length = list.as_ref().map_or(0, web_sys::NodeList::length);
    (0..length).filter_map(move |index| {
        list.as_ref()?
            .item(index)?
            .dyn_into::<web_sys::Element>()
            .ok()
    })
}

fn id_of(element: &web_sys::Element) -> Option<u64> {
    element.get_attribute(ID_ATTRIBUTE)?.parse().ok()
}

pub(crate) fn patch_attribute(node: &web_sys::Element, name: &str, value: Option<&str>) {
    if node.get_attribute(name).as_deref() == value {
        return;
    }
    crate::reconcile::set_or_clear(node, name, value);
}

/// Brings `live`'s children in line with `wanted`'s, keeping every node that is already the one wanted: text is rewritten in its node, an element of the same kind keeps its place and has its attributes patched, and only a node of another kind is replaced.
///
/// `wanted` is a detached element the caller built the content into, so what is compared is what the browser made of both.
pub(crate) fn morph_children(live: &web_sys::Node, wanted: &web_sys::Node) {
    let wanted_children = wanted.child_nodes();
    let count = wanted_children.length();
    for index in 0..count {
        let Some(want) = wanted_children.item(index) else {
            continue;
        };
        match live.child_nodes().item(index) {
            Some(have) if same_kind(&have, &want) => morph_node(&have, &want),
            Some(have) => {
                if let Ok(copy) = want.clone_node_with_deep(true) {
                    let _ = live.replace_child(&copy, &have);
                }
            }
            None => {
                if let Ok(copy) = want.clone_node_with_deep(true) {
                    let _ = live.append_child(&copy);
                }
            }
        }
    }
    crate::reconcile::truncate(live, count);
}

fn same_kind(have: &web_sys::Node, want: &web_sys::Node) -> bool {
    if have.node_type() != want.node_type() {
        return false;
    }
    match (
        have.dyn_ref::<web_sys::Element>(),
        want.dyn_ref::<web_sys::Element>(),
    ) {
        (Some(have), Some(want)) => {
            have.local_name() == want.local_name() && have.namespace_uri() == want.namespace_uri()
        }
        _ => true,
    }
}

fn morph_node(have: &web_sys::Node, want: &web_sys::Node) {
    match (
        have.dyn_ref::<web_sys::Element>(),
        want.dyn_ref::<web_sys::Element>(),
    ) {
        (Some(have_element), Some(want_element)) => {
            morph_attributes(have_element, want_element);
            morph_children(have, want);
        }
        _ => {
            let value = want.node_value();
            if have.node_value() != value {
                have.set_node_value(value.as_deref());
            }
        }
    }
}

fn morph_attributes(have: &web_sys::Element, want: &web_sys::Element) {
    let present = have.attributes();
    let stale: Vec<String> = (0..present.length())
        .filter_map(|index| present.item(index))
        .map(|attribute| attribute.name())
        .filter(|name| !want.has_attribute(name))
        .collect();
    for name in stale {
        let _ = have.remove_attribute(&name);
    }
    let wanted = want.attributes();
    for attribute in (0..wanted.length()).filter_map(|index| wanted.item(index)) {
        patch_attribute(have, &attribute.name(), Some(&attribute.value()));
    }
}
