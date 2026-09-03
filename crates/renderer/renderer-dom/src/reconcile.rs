//! Turning one frame of draw commands into the document it describes.
//!
//! The reconcile is keyed by [`ElementId`](renderer_core::ElementId), which is the layout node the widget was
//! built with: it lives as long as the widget, so a box that only moved is *moved*, and only a box that is
//! genuinely new is created. Nothing here diffs strings against the DOM — the last style written is kept
//! beside the node, because reading a property back out of the browser is the expensive direction.
//!
//! A box is a box, but not everything a box paints is one. Three things arrive inside an element: its own
//! background, which is CSS; child boxes, which the browser lays out; and paint that is neither — a caret, a
//! selection band, a scrollbar. The last of those become positioned children, in the order they were drawn,
//! so what covered what on a canvas covers the same thing here.

use geometry_core::Rect;
use renderer_core::{DrawCommand, Element, Role};
use rustc_hash::FxHashMap;

use crate::paint;
use crate::vector::Drawing;

const SVG_NS: &str = "http://www.w3.org/2000/svg";

/// Marks the element the app fills, so the reset below reaches its boxes and nothing else on the page.
const HOST_ATTRIBUTE: &str = "data-telar";
const RESET_ID: &str = "telar-reset";

/// Where a box was told to be, beside where the browser put it.
///
/// Written only when the page asks for it, because the whole claim of this backend is that the two agree —
/// and a claim nothing checks is a claim that quietly stops being true. Off by default: it is an attribute
/// written per box per frame, which is exactly the cost this reconcile exists to avoid.
const AUDIT_ATTRIBUTE: &str = "data-telar-rect";
const AUDIT_QUERY: &str = "telar-audit";

fn audit_requested() -> bool {
    web_sys::window()
        .and_then(|window| window.location().search().ok())
        .is_some_and(|search| search.contains(AUDIT_QUERY))
}

/// What a document brings to an element that Telar never asked for: a button's border and its own font, a
/// heading's margins, a link's colour and underline. A widget's style is the whole of what its box looks
/// like, and the browser's idea of it is the difference between what layout computed and what the page shows
/// — a button's 2px frame made every row of a list four pixels taller than the rect hit-testing reads.
///
/// One rule rather than a declaration per box per frame, and the base font is the one the measurer assumes,
/// so a paragraph is drawn in the face it was measured in.
const RESET: &str = "[data-telar]{font:400 16px sans-serif}\
[data-telar] *{margin:0;border:0;padding:0;background:none;font:inherit;color:inherit;\
text-align:inherit;text-decoration:none;box-sizing:border-box;appearance:none;\
-webkit-appearance:none;outline:none}";

fn install_reset(document: &web_sys::Document) {
    if document.get_element_by_id(RESET_ID).is_some() {
        return;
    }
    let Some(head) = document.head() else {
        return;
    };
    let Ok(style) = document.create_element("style") else {
        return;
    };
    let _ = style.set_attribute("id", RESET_ID);
    style.set_text_content(Some(RESET));
    let _ = head.append_child(style.as_ref());
}

/// One element the document is currently showing.
struct Live {
    node: web_sys::Element,
    /// The tag it was created with. A box whose role changes needs a different element, not a new attribute.
    tag: &'static str,
    /// What was last written to its `style` attribute, so an unchanged frame writes nothing.
    style: String,
    text: String,
    /// The markup last written into a drawing element.
    drawn: String,
    /// The paint this box carries that is not a box, as the children standing in for it.
    pieces: Vec<Piece>,
    /// What was last said about what this box *is*, so an unchanged frame writes no attributes.
    described: Described,
}

/// What a box is, as the attributes that say so.
#[derive(Default, PartialEq)]
struct Described {
    role: Option<&'static str>,
    label: Option<String>,
    link: Option<String>,
    hidden: bool,
}

/// Writes an attribute, or takes it off where there is nothing to say. Removing matters as much as setting:
/// a box that stops being a link keeps sending the reader somewhere until the `href` goes.
fn set_or_clear(node: &web_sys::Element, name: &str, value: Option<&str>) {
    match value {
        Some(value) => {
            let _ = node.set_attribute(name, value);
        }
        None => {
            let _ = node.remove_attribute(name);
        }
    }
}

/// One thing an element paints inside itself that the browser has to place rather than lay out.
struct Piece {
    node: web_sys::Element,
    style: String,
    text: String,
}

/// A piece as it is collected, before the element it belongs to is closed.
enum Painted {
    Rect {
        rect: Rect,
        style: String,
    },
    Text {
        rect: Rect,
        style: String,
        text: String,
    },
}

/// What is being assembled while the walk is inside one element.
struct Open {
    id: u64,
    /// Where layout put the box, so paint that *is* the box can be told from paint that is inside it.
    box_rect: Rect,
    /// Set for a box whose content is drawn rather than laid out; everything inside it goes here.
    drawing: Option<Drawing>,
    style: String,
    /// Whether the element's own background has been taken, so a box painted twice keeps the first.
    painted: bool,
    /// How many child boxes have been put in place, and therefore where the next one belongs.
    placed: u32,
    pieces: Vec<Painted>,
    /// A transform whose subject is not yet known: the box itself if its own paint turns up inside, and the
    /// boxes it wraps otherwise.
    moved: Option<String>,
}

impl Open {
    fn root() -> Self {
        Self {
            id: u64::MAX,
            box_rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            drawing: None,
            style: String::new(),
            // The host is not Telar's to paint: an application chose its size and its background before the
            // first frame, and a surface-wide fill would be written over both.
            painted: true,
            placed: 0,
            pieces: Vec::new(),
            moved: None,
        }
    }

    fn is_root(&self) -> bool {
        self.id == u64::MAX
    }
}

pub struct Reconciler {
    document: web_sys::Document,
    host: web_sys::HtmlElement,
    live: FxHashMap<u64, Live>,
    /// Ids seen this frame, so what is missing can be removed at the end.
    seen: Vec<u64>,
    open: Vec<Open>,
    /// Whether each box also carries the rect layout computed for it, for a test that compares the two.
    audit: bool,
}

impl Reconciler {
    pub fn new(host: web_sys::HtmlElement) -> Result<Self, String> {
        // Layout roots are placed in it absolutely, so it has to be what they are placed relative to.
        let _ = host.style().set_property("position", "relative");
        let document = host
            .owner_document()
            .ok_or_else(|| "the host element is not in a document".to_string())?;
        let _ = host.set_attribute(HOST_ATTRIBUTE, "");
        install_reset(&document);
        Ok(Self {
            audit: audit_requested(),
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
        self.open.push(Open::root());

        for command in commands {
            match command {
                DrawCommand::PushElement { element } => self.push(element),
                DrawCommand::PopElement => self.pop(),
                other => self.paint(other),
            }
        }

        // Close the host frame: anything left beyond what this frame placed is gone.
        if let Some(root) = self.open.pop() {
            truncate(self.host.as_ref(), root.placed);
        }
        self.retire();
    }

    /// Everything that is not an element boundary: what the open box paints.
    fn paint(&mut self, command: &DrawCommand) {
        let Some(open) = self.open.last_mut() else {
            return;
        };
        if let Some(drawing) = open.drawing.as_mut() {
            draw(drawing, command);
            return;
        }
        match command {
            DrawCommand::Rect { rect, style } => {
                if open.painted {
                    return;
                }
                // The box's own background is the one that *is* the box. Anything else is paint the widget
                // put inside it — a scroll area's bar, a field's selection — and folding that into the
                // background would spread one small mark over the whole element.
                if is_own_box(*rect, open.box_rect) {
                    open.painted = true;
                    // A matrix this box's own paint sits inside is the box's own transform, and now it is
                    // known to be: the boxes it also wraps are moved by moving the box.
                    if let Some(matrix) = open.moved.take() {
                        paint::declare(&mut open.style, "transform", &matrix);
                    }
                    paint::rect_style(style, &mut open.style);
                    return;
                }
                if open.is_root() {
                    return;
                }
                let mut css = String::new();
                paint::rect_style(style, &mut css);
                open.pieces.push(Painted::Rect {
                    rect: *rect,
                    style: css,
                });
            }
            DrawCommand::Text {
                text, rect, style, ..
            } => {
                if open.is_root() {
                    return;
                }
                let mut css = String::new();
                paint::text_style(style, &mut css);
                open.pieces.push(Painted::Text {
                    rect: *rect,
                    style: css,
                    text: text.to_string(),
                });
            }
            DrawCommand::PushLayer { opacity, .. } => {
                if *opacity < 1.0 {
                    paint::declare(&mut open.style, "opacity", &paint::round(*opacity));
                }
            }
            DrawCommand::PushClip { radius, .. } => {
                paint::declare(&mut open.style, "overflow", "hidden");
                if !radius.is_zero() {
                    paint::declare(
                        &mut open.style,
                        "border-radius",
                        &paint::px(radius.top_left),
                    );
                }
            }
            // A matrix moves whatever it wraps, and which that is only becomes clear inside it. A widget that
            // transforms itself draws its own box in there; a scroll area wraps its content and nothing
            // else, and a transform on the viewport would carry the viewport away with it.
            DrawCommand::PushMatrix { matrix } => {
                if *matrix != IDENTITY {
                    let [a, b, c, d, e, f] = matrix;
                    open.moved = Some(format!("matrix({a},{b},{c},{d},{e},{f})"));
                }
            }
            // Artwork and bitmaps reach a document as an SVG, which is what a drawing element is; one that
            // arrives in a box means a widget drew geometry without saying its box was a drawing.
            DrawCommand::Image { .. } | DrawCommand::Path { .. } | DrawCommand::Line { .. } => {
                tracing::debug!("a box painted geometry it did not declare itself a drawing for");
            }
            DrawCommand::PopMatrix => open.moved = None,
            DrawCommand::PopClip | DrawCommand::PopLayer => {}
            DrawCommand::PushElement { .. } | DrawCommand::PopElement => {}
        }
    }

    fn push(&mut self, element: &Element) {
        let tag = tag_of(&element.semantics.role);
        let node = self.element_for(element.id.0, tag);
        let drawing = matches!(element.semantics.role, Role::Drawing);
        let mut style = String::new();
        if drawing {
            // An `<svg>` is inline by default, which reserves a descender's worth of space under it that the
            // box it is standing in never asked for. The declarations follow, so a box that wants another
            // display still gets it.
            paint::declare(&mut style, "display", "block");
        }
        style.push_str(&element.layout);
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
        // A transform its parent is still holding is one that wraps this box rather than the parent: a
        // scroll area moves its content, and moving the viewport instead takes the panel off the page.
        if let Some(matrix) = self.open.last().and_then(|parent| parent.moved.clone()) {
            paint::declare(&mut style, "transform", &matrix);
        }
        self.describe(&node, element, tag);
        if self.audit {
            let rect = element.rect;
            let _ = node.set_attribute(
                AUDIT_ATTRIBUTE,
                &format!("{} {} {} {}", rect.x, rect.y, rect.width, rect.height),
            );
        }
        self.seen.push(element.id.0);
        self.open.push(Open {
            id: element.id.0,
            box_rect: element.rect,
            drawing: drawing.then(|| Drawing::new(element.id.0)),
            style,
            painted: false,
            placed: 0,
            pieces: Vec::new(),
            moved: None,
        });
    }

    /// Says what the box is, in whatever way the element it became does not already say it.
    ///
    /// A `<nav>` needs no `role="navigation"` — it *is* one, and duplicating it is noise a reader has to
    /// step over. Only the roles with no element of their own carry the attribute.
    fn describe(&mut self, node: &web_sys::Element, element: &Element, tag: &'static str) {
        let semantics = &element.semantics;
        let role = (tag == "div" || tag == "svg")
            .then(|| aria_role(semantics.role))
            .flatten();
        let label = semantics.label.as_deref();
        // Artwork nobody named is decoration, and a graphic with no accessible name is noise to read out.
        let hidden = semantics.role == Role::Drawing && label.is_none();
        let described = Described {
            role,
            label: label.map(str::to_string),
            link: semantics.link.as_deref().map(str::to_string),
            hidden,
        };
        let Some(live) = self.live.get_mut(&element.id.0) else {
            return;
        };
        if live.described == described {
            return;
        }
        set_or_clear(node, "role", described.role);
        set_or_clear(node, "aria-label", described.label.as_deref());
        set_or_clear(node, "href", described.link.as_deref());
        set_or_clear(node, "aria-hidden", described.hidden.then_some("true"));
        live.described = described;
    }

    fn pop(&mut self) {
        let Some(mut open) = self.open.pop() else {
            return;
        };
        // A single run of text is the box's own label, not something inside it: it becomes the element's
        // text and its style, which is what lets it be selected, found and read as part of the document.
        let inline_text = open.placed == 0
            && open.pieces.len() == 1
            && matches!(open.pieces[0], Painted::Text { .. });
        if inline_text && let Painted::Text { style, .. } = &open.pieces[0] {
            open.style.push_str(style);
        } else if !open.pieces.is_empty() && !open.style.contains("position:") {
            // Paint placed inside a box is placed against *that* box. Without this it is placed against
            // whatever the nearest positioned ancestor happens to be — the host, for most boxes — and a
            // field's own text went to the corner of the page.
            paint::declare(&mut open.style, "position", "relative");
        }

        let document = self.document.clone();
        let Some(live) = self.live.get_mut(&open.id) else {
            return;
        };
        // Everything the element ended up holding is known now, so the attribute is written once.
        if live.style != open.style {
            let _ = live.node.set_attribute("style", &open.style);
            live.style = open.style;
        }

        if let Some(drawing) = open.drawing {
            let markup = drawing.finish();
            if live.drawn != markup {
                live.node.set_inner_html(&markup);
                live.drawn = markup;
                live.text.clear();
                live.pieces.clear();
            }
        } else if inline_text {
            let Painted::Text { text, .. } = &open.pieces[0] else {
                unreachable!("inline_text is exactly this shape")
            };
            if live.text != *text {
                // Wipes the children with it, which is the point: the element carries the text itself now.
                live.node.set_text_content(Some(text));
                live.text = text.clone();
                live.pieces.clear();
            }
        } else {
            // Paint the box carries that is not a box goes after the boxes, which is the order it was drawn
            // in and therefore what it covers: a scroll area's bars are drawn over the content they scroll.
            live.text.clear();
            fill_pieces(&document, live, open.placed, &open.pieces);
        }

        let node = live.node.clone();
        self.place(node);
    }

    /// Puts `node` where the frame says it belongs inside the element being assembled, moving it only when
    /// it is not there already.
    fn place(&mut self, node: web_sys::Element) {
        let Some(parent_frame) = self.open.last_mut() else {
            return;
        };
        // A drawing owns everything inside it as markup; a box placed in one would be written over by the
        // next frame that changes the picture.
        if parent_frame.drawing.is_some() {
            return;
        }
        let index = parent_frame.placed;
        parent_frame.placed += 1;
        let parent: web_sys::Node = if parent_frame.is_root() {
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
    fn element_for(&mut self, id: u64, tag: &'static str) -> web_sys::Element {
        if let Some(live) = self.live.get(&id)
            && live.tag == tag
        {
            return live.node.clone();
        }
        let Some(node) = create(&self.document, tag) else {
            // Only reachable if the document refuses a tag this crate chose, which would be a bug here
            // rather than something an application can act on.
            tracing::error!("could not create a <{tag}>");
            return self.host.clone().into();
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
                drawn: String::new(),
                pieces: Vec::new(),
                described: Described::default(),
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

/// Whether a painted rect is the box it was painted in, in either of the two ways a widget can say so: a
/// box that draws its own frame knows where it is, and a leaf that draws inside itself starts at its corner.
fn is_own_box(rect: Rect, box_rect: Rect) -> bool {
    let same = |a: f32, b: f32| (a - b).abs() < 0.01;
    same(rect.width, box_rect.width)
        && same(rect.height, box_rect.height)
        && ((same(rect.x, box_rect.x) && same(rect.y, box_rect.y))
            || (same(rect.x, 0.0) && same(rect.y, 0.0)))
}

fn create(document: &web_sys::Document, tag: &'static str) -> Option<web_sys::Element> {
    // An `svg` made as an HTML element is an unknown tag that renders nothing: what makes it a drawing is
    // the namespace, not the name.
    if tag == "svg" {
        return document.create_element_ns(Some(SVG_NS), tag).ok();
    }
    document.create_element(tag).ok()
}

/// Brings the element's positioned children in line with what it painted this frame.
fn fill_pieces(document: &web_sys::Document, live: &mut Live, after: u32, pieces: &[Painted]) {
    // Anything past the boxes and the pieces is a child from a frame that had more of either.
    truncate(live.node.as_ref(), after + live.pieces.len() as u32);
    for (index, painted) in pieces.iter().enumerate() {
        let (rect, css, text) = match painted {
            Painted::Rect { rect, style } => (rect, style, ""),
            Painted::Text { rect, style, text } => (rect, style, text.as_str()),
        };
        let mut style = String::new();
        paint::declare(&mut style, "position", "absolute");
        paint::declare(&mut style, "left", &paint::px(rect.x));
        paint::declare(&mut style, "top", &paint::px(rect.y));
        paint::declare(&mut style, "width", &paint::px(rect.width.max(0.0)));
        paint::declare(&mut style, "height", &paint::px(rect.height.max(0.0)));
        style.push_str(css);

        if index == live.pieces.len() {
            let Ok(node) = document.create_element("div") else {
                return;
            };
            live.pieces.push(Piece {
                node,
                style: String::new(),
                text: String::new(),
            });
        }
        // Where the boxes end, in the order the paint was drawn — and only moved when it is not there.
        let at = after + index as u32;
        let node: &web_sys::Node = live.pieces[index].node.as_ref();
        let current = live.node.child_nodes().item(at);
        if !current
            .as_ref()
            .is_some_and(|existing| existing.is_same_node(Some(node)))
        {
            let _ = live.node.insert_before(node, current.as_ref());
        }
        let piece = &mut live.pieces[index];
        if piece.style != style {
            let _ = piece.node.set_attribute("style", &style);
            piece.style = style;
        }
        if piece.text != text {
            piece.node.set_text_content(Some(text));
            piece.text = text.to_string();
        }
    }
    while live.pieces.len() > pieces.len() {
        if let Some(extra) = live.pieces.pop() {
            extra.node.remove();
        }
    }
}

/// What one command adds to the picture an element is drawing.
fn draw(drawing: &mut Drawing, command: &DrawCommand) {
    match command {
        DrawCommand::Rect { rect, style } => drawing.rect(*rect, style),
        DrawCommand::Text {
            text, rect, style, ..
        } => drawing.text(text, *rect, style),
        DrawCommand::Path { data, style } => drawing.path(data, style),
        DrawCommand::Line { p1, p2, style } => drawing.line(*p1, *p2, style),
        DrawCommand::Image { data, rect, raster } => {
            if let Some(href) = crate::bitmap::href(data) {
                drawing.image(&href, *rect, *raster);
            }
        }
        DrawCommand::PushClip { rect, radius } => drawing.open_clip(*rect, *radius),
        DrawCommand::PushMatrix { matrix } => drawing.open_matrix(*matrix),
        DrawCommand::PushLayer { opacity, .. } => drawing.open_layer(*opacity),
        DrawCommand::PopClip | DrawCommand::PopMatrix | DrawCommand::PopLayer => {
            drawing.close_group()
        }
        DrawCommand::PushElement { .. } | DrawCommand::PopElement => {}
    }
}

/// Removes every child past `keep`, which is what a box that lost children leaves behind.
fn truncate(parent: &web_sys::Node, keep: u32) {
    while parent.child_nodes().length() > keep {
        let Some(extra) = parent.last_child() else {
            return;
        };
        let _ = parent.remove_child(&extra);
    }
}

/// The element a role *is*.
///
/// A `div` is not a role that failed: it is one the document has no element for, and
/// [`aria_role`] then says in an attribute what the tag could not. Preferring the element where there is one
/// is not decoration — an element carries the meaning to a reader, to a search index and to a stylesheet,
/// where an attribute reaches only the first.
fn tag_of(role: &Role) -> &'static str {
    match role {
        Role::Banner => "header",
        Role::Navigation => "nav",
        Role::Main => "main",
        Role::Complementary => "aside",
        Role::ContentInfo => "footer",
        Role::Article => "article",
        Role::Section => "section",
        Role::Form => "form",
        Role::Search => "search",
        Role::Button => "button",
        Role::Link => "a",
        Role::Drawing => "svg",
        Role::Heading(level) => match level {
            1 => "h1",
            2 => "h2",
            3 => "h3",
            4 => "h4",
            5 => "h5",
            _ => "h6",
        },
        _ => "div",
    }
}

/// The `role` attribute a box needs because the element it became does not carry its meaning.
///
/// `None` where the role *is* the element, and where there is nothing worth announcing: a plain group is a
/// `div`, and `role="group"` on every box in the tree is a reader reading out the scaffolding.
fn aria_role(role: Role) -> Option<&'static str> {
    match role {
        Role::Group => None,
        // A picture with a name; without one it is hidden instead, which `describe` decides.
        Role::Drawing => Some("img"),
        // A scroll area is a region a reader can be told about, but `scrollarea` is not an ARIA role and a
        // browser would ignore it.
        Role::ScrollArea => None,
        // `ul` and `li` are a pair with a content model — a `ul` may hold only `li` — and nothing here can
        // promise an author marked both. The ARIA roles are announced the same and are valid anywhere.
        Role::List => Some("list"),
        Role::ListItem => Some("listitem"),
        other => Some(other.as_str()),
    }
}
