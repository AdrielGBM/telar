//! Turning one frame of draw commands into the document it describes.
//!
//! The reconcile is keyed by [`ElementId`](renderer_core::ElementId), which is the layout node the widget was
//! built with: it lives as long as the widget, so a box that only moved is *moved*, and only a box that is
//! genuinely new is created. Nothing here diffs strings against the DOM — the last style written is kept
//! beside the node, because reading a property back out of the browser is the expensive direction.

use renderer_core::{DrawCommand, Element, Role};
use rustc_hash::FxHashMap;
use wasm_bindgen::JsCast;

use crate::paint;

/// One element the document is currently showing.
struct Live {
    node: web_sys::HtmlElement,
    /// The tag it was created with. A box whose role changes needs a different element, not a new attribute.
    tag: &'static str,
    /// What was last written to its `style` attribute, so an unchanged frame writes nothing.
    style: String,
    text: String,
}

/// What is being assembled while the walk is inside one element.
struct Open {
    id: u64,
    style: String,
    text: String,
    /// Whether the element's own background has been taken. The first `Rect` inside an element is the box's
    /// own — it is what `StyledContainer` draws before anything else — and later ones are decorations this
    /// backend does not yet place.
    painted: bool,
    /// How many children have been put in place, and therefore where the next one belongs.
    placed: u32,
}

pub struct Reconciler {
    document: web_sys::Document,
    host: web_sys::HtmlElement,
    live: FxHashMap<u64, Live>,
    /// Ids seen this frame, so what is missing can be removed at the end.
    seen: Vec<u64>,
    open: Vec<Open>,
}

impl Reconciler {
    pub fn new(host: web_sys::HtmlElement) -> Result<Self, String> {
        // Layout roots are placed in it absolutely, so it has to be what they are placed relative to.
        let _ = host.style().set_property("position", "relative");
        let document = host
            .owner_document()
            .ok_or_else(|| "the host element is not in a document".to_string())?;
        Ok(Self {
            document,
            host,
            live: FxHashMap::default(),
            seen: Vec::new(),
            open: Vec::new(),
        })
    }

    pub fn frame(&mut self, commands: &[DrawCommand]) {
        self.seen.clear();
        self.open.clear();
        // The host is the outermost frame, so a top-level element is placed in it by the same code that
        // places every other child.
        self.open.push(Open {
            id: u64::MAX,
            style: String::new(),
            text: String::new(),
            painted: true,
            placed: 0,
        });

        for command in commands {
            self.one(command);
        }

        // Close the host frame: anything left beyond what this frame placed is gone.
        if let Some(root) = self.open.pop() {
            truncate(self.host.as_ref(), root.placed);
        }
        self.retire();
    }

    fn one(&mut self, command: &DrawCommand) {
        match command {
            DrawCommand::PushElement { element } => self.push(element),
            DrawCommand::PopElement => self.pop(),
            DrawCommand::Rect { style, .. } => {
                let Some(open) = self.open.last_mut() else {
                    return;
                };
                if open.painted {
                    return;
                }
                open.painted = true;
                paint::rect_style(style, &mut open.style);
            }
            DrawCommand::Text { text, style, .. } => {
                let Some(open) = self.open.last_mut() else {
                    return;
                };
                open.text.push_str(text);
                paint::text_style(style, &mut open.style);
            }
            DrawCommand::PushLayer { opacity, .. } => {
                if let Some(open) = self.open.last_mut()
                    && *opacity < 1.0
                {
                    paint::declare(&mut open.style, "opacity", &format!("{opacity:.3}"));
                }
            }
            DrawCommand::PushClip { radius, .. } => {
                if let Some(open) = self.open.last_mut() {
                    paint::declare(&mut open.style, "overflow", "hidden");
                    if !radius.is_zero() {
                        paint::declare(
                            &mut open.style,
                            "border-radius",
                            &format!("{}px", radius.top_left),
                        );
                    }
                }
            }
            // A matrix inside an element is that element's own transform: the widget applied it to move or
            // scale itself, and CSS says the same thing in the same order.
            DrawCommand::PushMatrix { matrix } => {
                if let Some(open) = self.open.last_mut()
                    && *matrix != IDENTITY
                {
                    let [a, b, c, d, e, f] = matrix;
                    paint::declare(
                        &mut open.style,
                        "transform",
                        &format!("matrix({a},{b},{c},{d},{e},{f})"),
                    );
                }
            }
            // Pictures, vector art and rules are drawn by the raster backends and have no element of their
            // own yet; the box that holds them is still placed and still lays out.
            DrawCommand::Image { .. } | DrawCommand::Path { .. } | DrawCommand::Line { .. } => {}
            DrawCommand::PopClip | DrawCommand::PopMatrix | DrawCommand::PopLayer => {}
        }
    }

    fn push(&mut self, element: &Element) {
        let tag = tag_of(&element.semantics.role);
        let node = self.element_for(element.id.0, tag);
        let mut style = element.layout.to_string();
        // A box whose parent is the host is a layout root: the application computed it and placed it itself,
        // so there is no parent expressing where it goes and the declarations alone would stack them. This
        // is the one place the *computed* rect is used instead of what the box asked for.
        if self.open.len() == 1 {
            let rect = element.rect;
            paint::declare(&mut style, "position", "absolute");
            paint::declare(&mut style, "left", &paint::px(rect.x));
            paint::declare(&mut style, "top", &paint::px(rect.y));
            paint::declare(&mut style, "width", &paint::px(rect.width));
            paint::declare(&mut style, "height", &paint::px(rect.height));
        }
        if element.semantics.click_through {
            paint::declare(&mut style, "pointer-events", "none");
        }
        if let Some(label) = &element.semantics.label {
            let _ = node.set_attribute("aria-label", label);
        }
        self.seen.push(element.id.0);
        self.open.push(Open {
            id: element.id.0,
            style,
            text: String::new(),
            painted: false,
            placed: 0,
        });
    }

    fn pop(&mut self) {
        let Some(open) = self.open.pop() else {
            return;
        };
        let Some(live) = self.live.get_mut(&open.id) else {
            return;
        };
        // Everything the element ended up holding is known now, so the two attributes are written once.
        if live.style != open.style {
            let _ = live.node.set_attribute("style", &open.style);
            live.style = open.style;
        }
        // A box with children of its own never carries text: `set_text_content` would delete them.
        if open.placed == 0 {
            if live.text != open.text {
                live.node.set_text_content(Some(&open.text));
                live.text = open.text;
            }
        } else {
            truncate(live.node.as_ref(), open.placed);
            live.text.clear();
        }

        let node = live.node.clone();
        self.place(node);
    }

    /// Puts `node` where the frame says it belongs inside the element being assembled, moving it only when
    /// it is not there already.
    fn place(&mut self, node: web_sys::HtmlElement) {
        let Some(parent_frame) = self.open.last_mut() else {
            return;
        };
        let index = parent_frame.placed;
        parent_frame.placed += 1;
        let parent: web_sys::Node = if parent_frame.id == u64::MAX {
            self.host.clone().into()
        } else {
            match self.live.get(&parent_frame.id) {
                Some(live) => live.node.clone().into(),
                None => return,
            }
        };
        let current = parent.child_nodes().item(index);
        if current
            .as_ref()
            .is_some_and(|existing| existing.is_same_node(Some(node.as_ref())))
        {
            return;
        }
        let _ = parent.insert_before(node.as_ref(), current.as_ref());
    }

    /// The element for `id`, created if this is the first frame that mentions it — or recreated if what it
    /// means changed, since a role is a tag and a tag cannot be edited.
    fn element_for(&mut self, id: u64, tag: &'static str) -> web_sys::HtmlElement {
        if let Some(live) = self.live.get(&id)
            && live.tag == tag
        {
            return live.node.clone();
        }
        let node = self
            .document
            .create_element(tag)
            .ok()
            .and_then(|e| e.dyn_into::<web_sys::HtmlElement>().ok());
        let Some(node) = node else {
            // Only reachable if the document refuses a tag this crate chose, which would be a bug here
            // rather than something an application can act on.
            tracing::error!("could not create a <{tag}>");
            return self.host.clone();
        };
        if let Some(previous) = self.live.remove(&id) {
            previous.node.remove();
        }
        self.live.insert(
            id,
            Live {
                node: node.clone(),
                tag,
                style: String::new(),
                text: String::new(),
            },
        );
        node
    }

    /// Drops every element this frame did not mention.
    fn retire(&mut self) {
        if self.live.len() == self.seen.len() {
            return;
        }
        let seen: rustc_hash::FxHashSet<u64> = self.seen.iter().copied().collect();
        self.live.retain(|id, live| {
            if seen.contains(id) {
                return true;
            }
            live.node.remove();
            false
        });
    }
}

const IDENTITY: [f32; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// Removes every child past `keep`, which is what a box that lost children leaves behind.
fn truncate(parent: &web_sys::Node, keep: u32) {
    while parent.child_nodes().length() > keep {
        let Some(extra) = parent.last_child() else {
            return;
        };
        let _ = parent.remove_child(&extra);
    }
}

/// The tag a role is.
///
/// A role that maps to `div` is not a role that failed: it is one where the element carries the meaning
/// through an attribute instead, or one the document has no better word for.
fn tag_of(role: &Role) -> &'static str {
    match role {
        Role::Group | Role::ScrollArea | Role::Image => "div",
        Role::Button => "button",
        Role::Link(_) => "a",
        Role::TextInput => "div",
        Role::Heading(level) => match level {
            1 => "h1",
            2 => "h2",
            3 => "h3",
            4 => "h4",
            5 => "h5",
            _ => "h6",
        },
    }
}
