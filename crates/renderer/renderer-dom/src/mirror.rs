//! A frame drawn as pixels, told to assistive technology as a document nobody sees.

use std::cell::Cell;
use std::num::NonZeroU32;
use std::rc::Rc;

use platform_core::{AccessAction, AccessNode, Event, Role};
use rustc_hash::FxHashMap;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::Closure;

use crate::adopt::patch_attribute;
use crate::box_key::BoxKey;
use crate::document::{describe_access_node, tag_of, text_of_access_node};
use crate::paint::px;

/// Names the node an element stands for, by the id the snapshot gave it, on every element that stands for a control.
const NODE_ATTRIBUTE: &str = "data-telar-node";

const SVG_NAMESPACE: &str = "http://www.w3.org/2000/svg";

/// The elements stand where their nodes are drawn, so a reader that outlines what it reads, or finds what is under a finger, finds the drawing; and none of them is seen or takes the pointer, which is the drawing's.
const ROOT_STYLE: &str = "position:relative;width:0;height:0;margin:0;padding:0;border:0;opacity:0;pointer-events:none;overflow:visible";

/// A visually hidden document standing in for a frame that is drawn as pixels: an element for each node of the frame's accessibility snapshot, nested as the snapshot nests them and saying what each is in the words the document renderer gives the same box.
///
/// What a reader does to an element comes back as the platform's own events, as [`Event::AccessAction`]: focus landing on one asks its control to [take focus](AccessAction::Focus), a click on one to [activate](AccessAction::Activate). Focus going the other way follows too: while the keyboard is in the surface, the element of the control holding the app's focus holds the document's, which is what a reader announces.
///
/// No element is a Tab stop. The app walks its own order and decides at its ends, so a Tab it releases moves the browser's focus out of the surface rather than onto the next element here.
pub struct AccessMirror {
    root: web_sys::HtmlElement,
    shown: Vec<AccessNode>,
    elements: Vec<Shown>,
    /// Set while this moves the document's focus itself, so the move is not reported back as a reader's.
    moving: Rc<Cell<bool>>,
    _listeners: [Listener; 4],
}

/// The element standing for one node of the snapshot last shown.
struct Shown {
    element: web_sys::Element,
    tag: &'static str,
    id: Option<u64>,
    text: Option<web_sys::Text>,
}

impl AccessMirror {
    /// Puts an empty mirror into the document just before `drawing`, the element whose pixels it describes, so it sits where the drawing starts and is read before anything after it.
    pub fn before(drawing: &web_sys::Element) -> Result<Self, String> {
        let document = drawing
            .owner_document()
            .ok_or_else(|| "the drawing is not in a document".to_string())?;
        let parent = drawing
            .parent_node()
            .ok_or_else(|| "the drawing has no parent to put a mirror in".to_string())?;
        let root: web_sys::HtmlElement = document
            .create_element("div")
            .map_err(|_| "could not create the mirror".to_string())?
            .dyn_into()
            .map_err(|_| "a div is not an HTML element".to_string())?;
        let _ = root.set_attribute("style", ROOT_STYLE);
        parent
            .insert_before(&root, Some(drawing))
            .map_err(|_| "could not put the mirror beside the drawing".to_string())?;
        let moving = Rc::new(Cell::new(false));
        let target: &web_sys::EventTarget = root.as_ref();
        let focus_moving = Rc::clone(&moving);
        let listeners = [
            Listener::new(target, "focusin", move |event| {
                if focus_moving.get() {
                    return;
                }
                if let Some(id) = node_of(&event) {
                    post(id, AccessAction::Focus);
                }
            }),
            Listener::new(target, "click", |event| {
                // An element here only stands for a control: a `<button>` in a form would submit it and an `<a>` would leave the page.
                event.prevent_default();
                if let Some(id) = node_of(&event) {
                    post(id, AccessAction::Activate);
                }
            }),
            Listener::new(target, "keydown", keep_keys_from_activating),
            Listener::new(target, "keyup", keep_keys_from_activating),
        ];
        Ok(Self {
            root,
            shown: Vec::new(),
            elements: Vec::new(),
            moving,
            _listeners: listeners,
        })
    }

    /// Brings the mirror in line with `nodes`, a snapshot listed as [`AccessNode`] says: the element of a control that is still there is kept, so a reader resting on it stays where it is, and only what changed is written.
    pub fn show(&mut self, nodes: &[AccessNode]) {
        let Some(document) = self.root.owner_document() else {
            return;
        };
        let host = self.root.parent_element();
        let keyboard_inside = host.as_ref().is_some_and(|host| {
            document
                .active_element()
                .is_some_and(|active| host.contains(Some(active.as_ref())))
        });
        if self.shown != nodes {
            self.rewrite(&document, nodes);
        }
        if keyboard_inside {
            let held = nodes
                .iter()
                .position(|node| node.focused)
                .map(|index| self.elements[index].element.clone())
                .or(host);
            if let Some(held) = held {
                self.focus(&document, &held);
            }
        }
    }

    fn rewrite(&mut self, document: &web_sys::Document, nodes: &[AccessNode]) {
        let (elements, gone) = self.claim_elements(document, nodes);
        for shown in gone {
            self.release_focus_from(&shown.element);
            shown.element.remove();
        }
        let pointed_at = pointed_at(nodes);
        let row_counts = row_counts(nodes);
        for (index, (node, shown)) in nodes.iter().zip(&elements).enumerate() {
            let mut described = describe_access_node(node);
            described.is_active_descendant = node.id.is_some_and(|id| pointed_at.0.contains(&id));
            described.is_controlled = node.id.is_some_and(|id| pointed_at.1.contains(&id));
            described.row_count = row_counts[index];
            let key = BoxKey::from(node.id.unwrap_or_default());
            for (name, value) in described.attributes(key) {
                patch_attribute(&shown.element, name, value.as_deref());
            }
            let id = node.id.map(|id| id.to_string());
            patch_attribute(&shown.element, NODE_ATTRIBUTE, id.as_deref());
            let around = node.parent.and_then(|parent| nodes.get(parent));
            patch_attribute(&shown.element, "style", Some(&placement(node, around)));
        }
        let mut elements = elements;
        for (node, shown) in nodes.iter().zip(&mut elements) {
            write_text(document, shown, text_of_access_node(node));
        }

        let (top, held) = nesting(nodes);
        let elements_of = |indices: &[usize]| -> Vec<&web_sys::Element> {
            indices
                .iter()
                .map(|&index| &elements[index].element)
                .collect()
        };
        self.arrange(&self.root.clone().into(), None, elements_of(&top));
        for (shown, children) in elements.iter().zip(&held) {
            let node: web_sys::Node = shown.element.clone().into();
            self.arrange(&node, shown.text.as_ref(), elements_of(children));
        }
        self.elements = elements;
        self.shown = nodes.to_vec();
    }

    /// An element for each of `nodes`, in order — the one a control already had where it is still the same kind of element, otherwise the next one of its kind that stood for text, otherwise a new one — and the elements no node claimed.
    fn claim_elements(
        &mut self,
        document: &web_sys::Document,
        nodes: &[AccessNode],
    ) -> (Vec<Shown>, Vec<Shown>) {
        let mut controls: FxHashMap<u64, Shown> = FxHashMap::default();
        let mut unaddressed: Vec<Option<Shown>> = Vec::new();
        for shown in self.elements.drain(..) {
            match shown.id {
                Some(id) => {
                    controls.insert(id, shown);
                }
                None => unaddressed.push(Some(shown)),
            }
        }
        let mut gone = Vec::new();
        let mut claimed = Vec::with_capacity(nodes.len());
        for node in nodes {
            let tag = tag_of(&node.role);
            let kept = match node.id {
                Some(id) => controls.remove(&id).and_then(|shown| {
                    if shown.tag == tag {
                        Some(shown)
                    } else {
                        gone.push(shown);
                        None
                    }
                }),
                None => unaddressed
                    .iter_mut()
                    .find(|slot| slot.as_ref().is_some_and(|shown| shown.tag == tag))
                    .and_then(Option::take),
            };
            claimed.push(kept.unwrap_or_else(|| Shown::create(document, tag, node)));
        }
        gone.extend(controls.into_values());
        gone.extend(unaddressed.into_iter().flatten());
        (claimed, gone)
    }

    /// Makes `parent` hold `text`, then `children`, in that order and nothing else, moving only what is out of place.
    fn arrange(
        &self,
        parent: &web_sys::Node,
        text: Option<&web_sys::Text>,
        children: Vec<&web_sys::Element>,
    ) {
        let wanted: Vec<web_sys::Node> = text
            .map(|text| text.clone().into())
            .into_iter()
            .chain(children.into_iter().map(|child| child.clone().into()))
            .collect();
        let mut next = parent.first_child();
        for want in &wanted {
            if next
                .as_ref()
                .is_some_and(|next| next.is_same_node(Some(want)))
            {
                next = next.and_then(|next| next.next_sibling());
                continue;
            }
            self.release_focus_from(want);
            let _ = parent.insert_before(want, next.as_ref());
        }
        while let Some(stale) = next {
            next = stale.next_sibling();
            self.release_focus_from(&stale);
            let _ = parent.remove_child(&stale);
        }
    }

    /// Hands the document's focus to the surface before `node` is taken out of the document while holding it: a focused element that is moved or removed drops focus onto the page's body, and the keyboard with it.
    fn release_focus_from(&self, node: &web_sys::Node) {
        let Some(document) = self.root.owner_document() else {
            return;
        };
        let holds = document
            .active_element()
            .is_some_and(|active| node.contains(Some(active.as_ref())));
        if holds && let Some(host) = self.root.parent_element() {
            self.focus(&document, &host);
        }
    }

    fn focus(&self, document: &web_sys::Document, element: &web_sys::Element) {
        if document
            .active_element()
            .is_some_and(|active| active.is_same_node(Some(element.as_ref())))
        {
            return;
        }
        let Some(element) = element.dyn_ref::<web_sys::HtmlElement>() else {
            return;
        };
        let options = web_sys::FocusOptions::new();
        options.set_prevent_scroll(true);
        self.moving.set(true);
        let _ = element.focus_with_options(&options);
        self.moving.set(false);
    }
}

impl Drop for AccessMirror {
    fn drop(&mut self) {
        self.root.remove();
    }
}

impl Shown {
    fn create(document: &web_sys::Document, tag: &'static str, node: &AccessNode) -> Self {
        let element = match tag {
            "svg" => document.create_element_ns(Some(SVG_NAMESPACE), tag),
            tag => document.create_element(tag),
        }
        .expect("a document creates the elements it names");
        Self {
            element,
            tag,
            id: node.id,
            text: None,
        }
    }
}

/// Writes `text` as the element's own text, in the node it already has where it has one.
fn write_text(document: &web_sys::Document, shown: &mut Shown, text: Option<&str>) {
    match (text, &shown.text) {
        (None, _) => shown.text = None,
        (Some(text), Some(node)) => {
            if node.data() != text {
                node.set_data(text);
            }
        }
        (Some(text), None) => shown.text = Some(document.create_text_node(text)),
    }
}

/// Where a node's element stands: on the node's own rect, which is the window's, measured from the rect of the node it sits in, whose element is the one it is placed in.
fn placement(node: &AccessNode, around: Option<&AccessNode>) -> String {
    let (x, y) = around.map_or((0.0, 0.0), |around| (around.rect.x, around.rect.y));
    format!(
        "position:absolute;left:{};top:{};width:{};height:{};margin:0;padding:0;border:0;white-space:pre-wrap",
        px(node.rect.x - x),
        px(node.rect.y - y),
        px(node.rect.width.max(0.0)),
        px(node.rect.height.max(0.0)),
    )
}

/// The nodes the window holds directly, and those each node holds, by their places in `nodes`, in reading order.
fn nesting(nodes: &[AccessNode]) -> (Vec<usize>, Vec<Vec<usize>>) {
    let mut top = Vec::new();
    let mut held = vec![Vec::new(); nodes.len()];
    for (index, node) in nodes.iter().enumerate() {
        match node.parent.filter(|&parent| parent < index) {
            Some(parent) => held[parent].push(index),
            None => top.push(index),
        }
    }
    (top, held)
}

/// The ids of the nodes some node's cursor rests on, and of the nodes some node controls: each takes an `id` to be pointed at by.
fn pointed_at(nodes: &[AccessNode]) -> (Vec<u64>, Vec<u64>) {
    (
        nodes
            .iter()
            .filter_map(|node| node.active_descendant)
            .collect(),
        nodes.iter().filter_map(|node| node.controls).collect(),
    )
}

/// How many rows each grid has, as a row of it that says where it sits says: what a virtualised grid knows and does not build.
fn row_counts(nodes: &[AccessNode]) -> Vec<Option<NonZeroU32>> {
    let mut counts = vec![None; nodes.len()];
    for node in nodes {
        let Some(place) = node
            .position
            .filter(|_| node.role.positioning() == Some(platform_core::Positioning::GridRow))
        else {
            continue;
        };
        let mut above = node.parent;
        while let Some(at) = above {
            if nodes[at].role == Role::Grid {
                counts[at] = Some(place.size);
                break;
            }
            above = nodes[at].parent;
        }
    }
    counts
}

/// The control the element an event reached stands for, or the one nearest around it.
fn node_of(event: &web_sys::Event) -> Option<u64> {
    let element = event
        .target()?
        .dyn_into::<web_sys::Element>()
        .ok()?
        .closest(&format!("[{NODE_ATTRIBUTE}]"))
        .ok()??;
    element.get_attribute(NODE_ATTRIBUTE)?.parse().ok()
}

fn post(id: u64, action: AccessAction) {
    platform_core::post_event(Event::AccessAction { id, action });
}

/// Keeps a `<button>` or an `<a>` here from turning Enter or Space into a click of its own: the key reaches the app as a key, which activates the control it is focused on, and a click besides would activate it twice.
fn keep_keys_from_activating(event: web_sys::Event) {
    if let Some(key) = event.dyn_ref::<web_sys::KeyboardEvent>()
        && matches!(key.key().as_str(), "Enter" | " ")
    {
        event.prevent_default();
    }
}

/// One listener on the mirror, taken off when the mirror goes.
struct Listener {
    target: web_sys::EventTarget,
    event: &'static str,
    closure: Closure<dyn FnMut(web_sys::Event)>,
}

impl Listener {
    fn new(
        target: &web_sys::EventTarget,
        event: &'static str,
        handler: impl FnMut(web_sys::Event) + 'static,
    ) -> Self {
        let closure = Closure::<dyn FnMut(web_sys::Event)>::new(handler);
        let _ = target.add_event_listener_with_callback(event, closure.as_ref().unchecked_ref());
        Self {
            target: target.clone(),
            event,
            closure,
        }
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        let _ = self
            .target
            .remove_event_listener_with_callback(self.event, self.closure.as_ref().unchecked_ref());
    }
}
