//! Bringing the live document in line with the one a frame describes.
//!
//! What the document should be is worked out in `document.rs`, the same way a prerendered page is; this half only knows how to get there from what the last frame left. The reconcile is keyed by [`ElementId`](renderer_core::ElementId), which is the layout node the widget was built with: it lives as long as the widget, so a box that only moved is *moved*, and only a box that is genuinely new is created. Nothing here diffs strings against the DOM — the last style written is kept beside the node, because reading a property back out of the browser is the expensive direction.
//!
//! The one exception is the first frame on a host that was served a prerendered page: there the elements already exist, and are taken over rather than built (`adopt.rs`), which means reading each one once.

use std::rc::Rc;

use geometry_core::Rect;
use platform_core::consumed_keys::{CONSUMED_KEYS_ATTRIBUTE, FOCUS_BOX_ATTRIBUTE};
use renderer_core::{Color, DrawCommand, ImageData, Role, TextStyle};
use rustc_hash::FxHashMap;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::Closure;

use crate::document::{
    self, AUDIT_ATTRIBUTE, BoxNode, Content, Described, HOST_ATTRIBUTE, ID_ATTRIBUTE, Node,
    PaintNode, RESET, RESET_ID, Surface,
};
use crate::paint;

const SVG_NS: &str = "http://www.w3.org/2000/svg";

/// Where a box was told to be, beside where the browser put it.
///
/// Written only when the page asks for it, because the whole claim of this backend is that the two agree — and a claim nothing checks is a claim that quietly stops being true. Off by default: it is an attribute written per box per frame, which is exactly the cost this reconcile exists to avoid.
const AUDIT_QUERY: &str = "telar-audit";
/// The same request made of the host element, for a page that does not own its query string — and for the test that compares the two rects, which runs at whatever URL its harness serves it from.
const AUDIT_OPT_IN: &str = "data-telar-audit";

fn audit_requested(host: &web_sys::HtmlElement) -> bool {
    if host.has_attribute(AUDIT_OPT_IN) {
        return true;
    }
    web_sys::window()
        .and_then(|window| window.location().search().ok())
        .is_some_and(|search| search.contains(AUDIT_QUERY))
}

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
    /// What was last written to a picture's `<img>`.
    shown: Option<crate::picture::Shown>,
    /// Kept alive for a box that scrolls itself: dropping the closure unregisters the listener behind it.
    _scrolls: Option<Closure<dyn FnMut(web_sys::Event)>>,
    /// Taken over from the served page and not yet brought in line with a frame, so what it says is read off the element instead of assumed.
    adopted: bool,
    served_pieces: std::collections::VecDeque<web_sys::Element>,
}

/// Writes an attribute, or takes it off where there is nothing to say. Removing matters as much as setting: a box that stops being a link keeps sending the reader somewhere until the `href` goes.
pub(crate) fn set_or_clear(node: &web_sys::Element, name: &str, value: Option<&str>) {
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
    adopted: bool,
}

impl Piece {
    fn created(node: web_sys::Element) -> Self {
        Self {
            node,
            style: String::new(),
            text: String::new(),
            adopted: false,
        }
    }

    fn adopted(node: web_sys::Element) -> Self {
        Self {
            style: node.get_attribute("style").unwrap_or_default(),
            node,
            text: String::new(),
            adopted: true,
        }
    }

    /// Brings the piece's content in line with `text`, or with `runs` when it has spans, as the text of box `box_id`.
    fn show(
        &mut self,
        document: &web_sys::Document,
        text: &str,
        runs: Option<&[crate::runs::Run]>,
        box_id: u64,
    ) {
        let written = runs.map(crate::runs::signature);
        let wanted = written.as_deref().unwrap_or(text);
        if std::mem::take(&mut self.adopted) {
            morph_text(document, &self.node, text, runs, box_id);
        } else if self.text != wanted {
            write_text(document, &self.node, text, runs, box_id);
        }
        if self.text != wanted {
            self.text = wanted.to_string();
        }
    }
}

/// Turns one frame's command list into the document, reusing the elements the last frame left in place.
pub struct Reconciler {
    document: web_sys::Document,
    host: web_sys::HtmlElement,
    live: FxHashMap<u64, Live>,
    /// Ids seen this frame, so what is missing can be removed at the end.
    seen: Vec<u64>,
    /// Whether each box also carries the rect layout computed for it, for a test that compares the two.
    audit: bool,
    /// The surface background last written to the host, so an unchanged frame writes nothing.
    background: String,
    /// The boxes standing in for paint the frame carries at its top level, in the order it was drawn.
    root_paint: Vec<Piece>,
    /// Whether a box claimed the keyboard this frame, so a frame where none did can put it back.
    claimed_focus: bool,
    /// The one editable element the browser will type into, parked over whichever field holds the keyboard.
    entry: Option<crate::entry::TextEntry>,
    /// The field that holds the keyboard this frame, measured once everything is in the document.
    entry_target: Option<(web_sys::Element, bool)>,
    /// The box that holds the keyboard this frame, when it is not a field; focused once it is in the document.
    focus_target: Option<web_sys::HtmlElement>,
    _follows_focus: Option<FocusFollower>,
    _follows_links: Option<crate::links::LinkFollower>,
    /// The document's own scroll, while a box that is the surface's primary scroll holds it.
    document_scroll: Option<crate::document_scroll::DocumentScroll>,
    /// What the page was served with inside the host before the app ran, until the first frame has taken it over.
    served: Option<crate::adopt::Served>,
}

/// The live document, as the walk asks it things.
struct LiveSurface<'a> {
    host: &'a web_sys::HtmlElement,
    document_scroll: &'a mut Option<crate::document_scroll::DocumentScroll>,
    /// Where the surface's origin is in the viewport this frame, read once and only when something is placed against it.
    origin: Option<(f32, f32)>,
}

impl Surface for LiveSurface<'_> {
    fn hold_document_scroll(&mut self, id: u64) {
        match self.document_scroll.as_ref() {
            Some(held) => held.follow(id),
            None => {
                *self.document_scroll =
                    Some(crate::document_scroll::DocumentScroll::hold(self.host, id));
            }
        }
    }

    fn fixed_origin(&mut self) -> Option<(f32, f32)> {
        let held = self.document_scroll.as_ref()?;
        Some(*self.origin.get_or_insert_with(|| held.surface_origin()))
    }

    fn host_origin(&mut self) -> (f32, f32) {
        let rect = self.host.get_bounding_client_rect();
        (rect.left() as f32, rect.top() as f32)
    }

    fn image_href(&mut self, data: &ImageData) -> Option<Rc<str>> {
        crate::bitmap::href(data)
    }

    fn baseline(&mut self, style: &TextStyle) -> f32 {
        crate::metrics::baseline(style)
    }
}

impl Reconciler {
    pub fn new(host: web_sys::HtmlElement) -> Result<Self, String> {
        // Layout roots are placed in it absolutely, so it has to be what they are placed relative to.
        set_host_property(&host, "position", "relative");
        let document = host
            .owner_document()
            .ok_or_else(|| "the host element is not in a document".to_string())?;
        crate::adopt::patch_attribute(&host, HOST_ATTRIBUTE, Some(""));
        install_reset(&document);
        let served = Some(crate::adopt::Served::index(&host));
        let entry = crate::entry::TextEntry::new(&document, &host);
        let follows_focus = follow_focus(&host);
        Ok(Self {
            _follows_focus: follows_focus,
            _follows_links: crate::links::follow_links(&host),
            document_scroll: None,
            served,
            audit: audit_requested(&host),
            background: String::new(),
            root_paint: Vec::new(),
            claimed_focus: false,
            entry,
            entry_target: None,
            focus_target: None,
            document,
            host,
            live: FxHashMap::default(),
            seen: Vec::new(),
        })
    }

    pub fn frame(&mut self, commands: &[DrawCommand], clear: Option<Color>) {
        let frame = document::describe_frame(
            commands,
            clear,
            &mut LiveSurface {
                host: &self.host,
                document_scroll: &mut self.document_scroll,
                origin: None,
            },
            self.audit,
        );
        self.paint_host(frame.background);
        if frame.isolate_host {
            set_host_property(&self.host, "isolation", "isolate");
        }
        self.seen.clear();

        let host: web_sys::Node = self.host.clone().into();
        let mut root_painted = 0usize;
        let mut placed = 0u32;
        for child in &frame.children {
            let node = match child {
                Node::Box(node) => Some(self.apply(node, &host)),
                Node::Paint(paint) => self.paint_at_root(root_painted, paint).inspect(|_| {
                    root_painted += 1;
                }),
            };
            if let Some(node) = node {
                place(&host, placed, &node);
                placed += 1;
            }
        }

        while self.root_paint.len() > root_painted {
            if let Some(extra) = self.root_paint.pop() {
                extra.node.remove();
            }
        }
        // Anything left beyond what this frame placed is gone — except the one editable element the browser types into, which is put in its place first so that it is moved rather than swept.
        if let Some(entry) = self.entry.as_ref() {
            entry.settle(&self.host, placed);
        }
        let entry = u32::from(self.entry.is_some());
        truncate(self.host.as_ref(), placed + entry);
        let adopting = self.served.take().map(crate::adopt::Served::finish);
        self.retire();
        if frame.primary.is_none() {
            self.document_scroll = None;
        }
        if let Some(held) = self.document_scroll.as_ref() {
            held.arrive_within(frame.arrival_margin);
            held.keep_arrival();
        }
        self.keep_the_keyboard();
        if adopting.is_some() {
            self.follow_served_focus();
        }
    }

    /// Tells the app which box a reader focused on the served page before it ran, so its focus starts where the document's already is.
    fn follow_served_focus(&self) {
        let Some(active) = self.document.active_element() else {
            return;
        };
        if !self.host.contains(Some(active.as_ref())) {
            return;
        }
        if let Some(box_id) = active
            .get_attribute(FOCUS_BOX_ATTRIBUTE)
            .and_then(|id| id.parse::<u64>().ok())
        {
            platform_core::post_event(platform_core::Event::BoxFocused { box_id });
        }
    }

    /// The surface's own background, as a property of the element the app fills.
    ///
    /// Every other backend is handed this as the colour to clear to before anything is drawn. A document has no clear, and nothing else in the frame stands for it: an application states its background once, in `clear_color`, and its root box paints panels and text over a surface it never fills itself. Dropped, that surface was whatever the page happened to be — white, for the generated one — so a dark theme arrived as its own dark panels on a white page, and switching themes recoloured everything except the thing behind it.
    ///
    /// Set property by property rather than through the `style` attribute: the host also carries the positioning the reconcile needs and whatever cursor the app last asked for, and writing the attribute whole would take both off.
    ///
    /// The scheme is told from the colour, not from the media query, so an application whose theme was picked by hand rather than followed from the system still gets a browser dressed to match it.
    fn paint_host(&mut self, background: Option<Color>) {
        let declared = background.map(paint::color).unwrap_or_default();
        if self.background == declared {
            return;
        }
        match background {
            Some(color) => {
                set_host_property(&self.host, "background-color", &declared);
                set_host_property(&self.host, "color-scheme", paint::scheme_of(color));
            }
            None => {
                let style = self.host.style();
                let _ = style.remove_property("background-color");
                let _ = style.remove_property("color-scheme");
            }
        }
        self.background = declared;
    }

    /// The element standing for the `index`th piece of paint the frame carries at its own level, reused from the frame before when there was one.
    fn paint_at_root(&mut self, index: usize, paint: &PaintNode) -> Option<web_sys::Element> {
        if index == self.root_paint.len() {
            let piece = match self
                .served
                .as_mut()
                .and_then(|served| served.next_root_paint())
            {
                Some(node) => Piece::adopted(node),
                None => Piece::created(self.document.create_element("div").ok()?),
            };
            self.root_paint.push(piece);
        }
        let piece = &mut self.root_paint[index];
        if piece.style != paint.style {
            let _ = piece.node.set_attribute("style", &paint.style);
            piece.style = paint.style.clone();
        }
        piece.show(&self.document, &paint.text, None, 0);
        Some(piece.node.clone())
    }

    /// Brings one box's element, and everything inside it, in line with `node`. `parent` is where the box belongs, which a served element has to be in already to be taken over.
    fn apply(&mut self, node: &BoxNode, parent: &web_sys::Node) -> web_sys::Element {
        let element = self.element_for(node.id, node.tag, node.scrolls, parent);
        self.describe(&element, node);
        if let Some((picture, width)) = &node.picture
            && let Some(live) = self.live.get_mut(&node.id)
        {
            crate::picture::show(&element, picture, *width, &mut live.shown);
        }
        match self.document_scroll.as_ref() {
            Some(held) if node.primary => held.scroll_as_asked(node.scroll_to),
            _ => settle_scroll(&element, node.scroll_to),
        }
        let audit = node
            .audit
            .map(|rect| format!("{} {} {} {}", rect.x, rect.y, rect.width, rect.height));
        if audit.is_some() || self.live.get(&node.id).is_some_and(|live| live.adopted) {
            crate::adopt::patch_attribute(&element, AUDIT_ATTRIBUTE, audit.as_deref());
        }
        self.seen.push(node.id);

        let own: web_sys::Node = element.clone().into();
        if let Content::Children { boxes, .. } = &node.content {
            for (index, child) in boxes.iter().enumerate() {
                let child = self.apply(child, &own);
                place(&own, index as u32, &child);
            }
        }

        let document = self.document.clone();
        let Some(live) = self.live.get_mut(&node.id) else {
            return element;
        };
        let adopted = std::mem::take(&mut live.adopted);
        // Everything the element ended up holding is known now, so the attribute is written once.
        if live.style != node.style {
            let _ = live.node.set_attribute("style", &node.style);
            live.style = node.style.clone();
        }
        match &node.content {
            Content::Drawing(markup) => {
                if adopted {
                    morph_drawing(&document, &live.node, markup);
                } else if live.drawn != *markup {
                    live.node.set_inner_html(markup);
                }
                if live.drawn != *markup {
                    live.drawn = markup.clone();
                    live.text.clear();
                    live.pieces.clear();
                }
            }
            Content::Text { text, runs } => {
                let wanted = runs
                    .as_deref()
                    .map(crate::runs::signature)
                    .unwrap_or_else(|| text.clone());
                if adopted {
                    morph_text(&document, &live.node, text, runs.as_deref(), node.id);
                } else if live.text != wanted {
                    write_text(&document, &live.node, text, runs.as_deref(), node.id);
                }
                if live.text != wanted {
                    live.text = wanted;
                    live.pieces.clear();
                }
            }
            Content::Children { boxes, pieces } => {
                live.text.clear();
                fill_pieces(
                    &document,
                    live,
                    node.id,
                    boxes.len() as u32,
                    pieces,
                    self.served.as_mut(),
                );
            }
        }
        element
    }

    /// Keeps the document's focus on the box Telar focused, and the keyboard inside the app.
    ///
    /// Key listeners sit on the host, and an event only reaches them by bubbling *up* to it — so focus that escapes to `<body>` takes the whole keyboard with it, silently. It escapes on its own: an element this reconcile replaces takes its focus down with it, and the browser hands it to the document. Run once everything is in the document, because a detached element can be neither measured nor focused.
    ///
    /// Focus a person moved elsewhere on the page — tabbing past the app's last control, clicking a link beside it — is left where it went: taking it back every frame is a keyboard trap.
    fn keep_the_keyboard(&mut self) {
        let focus_target = self.focus_target.take();
        let claimed = std::mem::take(&mut self.claimed_focus);
        let ours = self.keyboard_is_ours();
        // A field is typed into through the entry, placed against what the browser did with the field.
        if let Some((node, multiline)) = self.entry_target.take() {
            let host = self.host.get_bounding_client_rect();
            let box_rect = node.get_bounding_client_rect();
            let placed = Rect::new(
                (box_rect.x() - host.x()) as f32,
                (box_rect.y() - host.y()) as f32,
                box_rect.width() as f32,
                box_rect.height() as f32,
            );
            let label = node.get_attribute("aria-label");
            let keys = node.get_attribute(CONSUMED_KEYS_ATTRIBUTE);
            if let Some(entry) = self.entry.as_mut() {
                let field = crate::entry::Field {
                    rect: placed,
                    multiline,
                    label: label.as_deref(),
                    keys: keys.as_deref(),
                    element: node.dyn_into::<web_sys::HtmlElement>().ok(),
                };
                entry.park(field, ours);
            }
            return;
        }
        if let Some(target) = focus_target {
            let active = self.document.active_element();
            if ours && active.as_ref() != Some(target.as_ref()) {
                let _ = target.focus();
            }
        }
        if claimed {
            return;
        }
        if let Some(entry) = self.entry.as_mut() {
            entry.release();
        }
        let inside = self
            .document
            .active_element()
            .is_some_and(|active| self.host.contains(Some(active.as_ref())));
        if ours && !inside {
            let _ = self.host.focus();
        }
    }

    /// Whether the document's focus is the app's to move: inside it, or fallen to nowhere in particular.
    fn keyboard_is_ours(&self) -> bool {
        let Some(active) = self.document.active_element() else {
            return true;
        };
        self.host.contains(Some(active.as_ref()))
            || self
                .document
                .body()
                .is_some_and(|body| body.is_same_node(Some(active.as_ref())))
    }

    /// Writes what `node` says the box is, unless the element already says it, and notes a box that holds the keyboard.
    fn describe(&mut self, element: &web_sys::Element, node: &BoxNode) {
        // Every frame, whatever else is skipped: a frame that did not answer where the keyboard is read as one where no box held it, and took the entry out from under the field being typed into.
        if node.described.focused {
            self.claimed_focus = true;
            match node.role {
                // A browser accepts characters for an editable element, and the box a person sees is not one.
                Role::TextInput | Role::MultilineTextInput => {
                    self.entry_target =
                        Some((element.clone(), node.role == Role::MultilineTextInput));
                }
                // The document has a focus of its own, and two that disagree is one interface the keyboard and the screen reader read differently.
                _ => self.focus_target = element.clone().dyn_into::<web_sys::HtmlElement>().ok(),
            }
        }
        let Some(live) = self.live.get_mut(&node.id) else {
            return;
        };
        if live.adopted {
            for (name, value) in node.described.attributes(node.id) {
                crate::adopt::patch_attribute(element, name, value.as_deref());
            }
        } else if live.described != node.described {
            for (name, value) in node.described.attributes(node.id) {
                set_or_clear(element, name, value.as_deref());
            }
        } else {
            return;
        }
        live.described = node.described.clone();
    }

    /// The element for `id`: on the first frame the served one, while it still fits under `parent`; otherwise created if this is the first frame that mentions it — or recreated if what it means changed, since a role is a tag and a tag cannot be edited.
    fn element_for(
        &mut self,
        id: u64,
        tag: &'static str,
        scrolls: bool,
        parent: &web_sys::Node,
    ) -> web_sys::Element {
        if let Some(live) = self.live.get(&id)
            && live.tag == tag
        {
            return live.node.clone();
        }
        let claimed = self
            .served
            .as_mut()
            .and_then(|served| served.claim(id, tag, parent));
        let adopted = claimed.is_some();
        let Some(node) = claimed.or_else(|| create(&self.document, tag, id)) else {
            // Only reachable if the document refuses a tag this crate chose, which would be a bug here rather than something an application can act on.
            tracing::error!("could not create a <{tag}>");
            return self.host.clone().into();
        };
        if let Some(previous) = self.live.remove(&id) {
            previous.node.remove();
        }
        // A box that scrolls itself has to say where it ended up, or hit-testing and every anchored overlay keep reading an offset that stopped being true the moment the compositor moved it.
        let scrolls = scrolls.then(|| watch_scroll(&node, id)).flatten();
        // A reader may have scrolled the served box before the app ran, and the app has to start from where they left it.
        if adopted && scrolls.is_some() && (node.scroll_left() != 0 || node.scroll_top() != 0) {
            report_scroll(&node, id);
        }
        let (style, served_pieces) = match adopted {
            true => (
                node.get_attribute("style").unwrap_or_default(),
                crate::adopt::paint_children(&node),
            ),
            false => Default::default(),
        };
        self.live.insert(
            id,
            Live {
                node: node.clone(),
                tag,
                style,
                text: String::new(),
                drawn: String::new(),
                pieces: Vec::new(),
                described: Described::default(),
                shown: None,
                _scrolls: scrolls,
                adopted,
                served_pieces,
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

/// Puts `node` at `index` among `parent`'s children, moving it only when it is not there already.
fn place(parent: &web_sys::Node, index: u32, node: &web_sys::Element) {
    let current = parent.child_nodes().item(index);
    if current
        .as_ref()
        .is_some_and(|existing| existing.is_same_node(Some(node.as_ref())))
    {
        return;
    }
    let _ = parent.insert_before(node.as_ref(), current.as_ref());
}

fn create(document: &web_sys::Document, tag: &'static str, id: u64) -> Option<web_sys::Element> {
    // An `svg` made as an HTML element is an unknown tag that renders nothing: what makes it a drawing is the namespace, not the name.
    let element = match tag {
        "svg" => document.create_element_ns(Some(SVG_NS), tag).ok()?,
        _ => document.create_element(tag).ok()?,
    };
    let _ = element.set_attribute(ID_ATTRIBUTE, &id.to_string());
    // Written here rather than by `describe`, which writes nothing for a box that says nothing: an `<img>` with no `alt` at all is one a reader announces by its address.
    if tag == "img" {
        let _ = element.set_attribute("alt", "");
    }
    Some(element)
}

/// Brings the element's positioned children in line with what it painted this frame.
///
/// While `served` is still being taken over, a new piece is the next served one, and what lies past the pieces is swept once the frame is done rather than here.
fn fill_pieces(
    document: &web_sys::Document,
    live: &mut Live,
    box_id: u64,
    after: u32,
    pieces: &[PaintNode],
    served: Option<&mut crate::adopt::Served>,
) {
    // Anything past the boxes and the pieces is a child from a frame that had more of either.
    if served.is_none() {
        truncate(live.node.as_ref(), after + live.pieces.len() as u32);
    }
    for (index, painted) in pieces.iter().enumerate() {
        if index == live.pieces.len() {
            let piece = match live.served_pieces.pop_front() {
                Some(node) => Piece::adopted(node),
                None => {
                    let Ok(node) = document.create_element("div") else {
                        return;
                    };
                    Piece::created(node)
                }
            };
            // Paint, not content: a caret, a selection band, a scrollbar's thumb. Out of the accessibility tree entirely, because in it they are children — and a role that comes with a content model counts them. A `role="list"` whose scrollbar is one of its children has a child that is not a `listitem`, which is exactly what an audit reports and a reader walks into.
            crate::adopt::patch_attribute(&piece.node, "role", Some("presentation"));
            live.pieces.push(piece);
        }
        // Where the boxes end, in the order the paint was drawn — and only moved when it is not there.
        let parent: web_sys::Node = live.node.clone().into();
        place(&parent, after + index as u32, &live.pieces[index].node);
        let piece = &mut live.pieces[index];
        if piece.style != painted.style {
            let _ = piece.node.set_attribute("style", &painted.style);
            piece.style = painted.style.clone();
        }
        piece.show(document, &painted.text, painted.runs.as_deref(), box_id);
    }
    while live.pieces.len() > pieces.len() {
        if let Some(extra) = live.pieces.pop() {
            extra.node.remove();
        }
    }
    live.served_pieces.clear();
    if let Some(served) = served {
        served.sweep_past(live.node.clone().into(), after + pieces.len() as u32);
    }
}

/// Replaces `node`'s children with `text`, or with `runs` when it has spans. Wipes the children with it, which is the point: the element carries the text itself now.
fn write_text(
    document: &web_sys::Document,
    node: &web_sys::Element,
    text: &str,
    runs: Option<&[crate::runs::Run]>,
    box_id: u64,
) {
    match runs {
        Some(runs) => crate::runs::write(document, node, runs, box_id),
        None => node.set_text_content(Some(text)),
    }
}

/// The same content brought into a served element, keeping every text node and inline element already there that says the same.
fn morph_text(
    document: &web_sys::Document,
    node: &web_sys::Element,
    text: &str,
    runs: Option<&[crate::runs::Run]>,
    box_id: u64,
) {
    let Ok(wanted) = document.create_element("div") else {
        write_text(document, node, text, runs, box_id);
        return;
    };
    write_text(document, &wanted, text, runs, box_id);
    crate::adopt::morph_children(node.as_ref(), wanted.as_ref());
}

/// A drawing's markup brought into a served `<svg>`, keeping every shape already there: a picture the page could not carry whole gains only what it lacked.
fn morph_drawing(document: &web_sys::Document, node: &web_sys::Element, markup: &str) {
    let Ok(wanted) = document.create_element_ns(Some(SVG_NS), "svg") else {
        node.set_inner_html(markup);
        return;
    };
    wanted.set_inner_html(markup);
    crate::adopt::morph_children(node.as_ref(), wanted.as_ref());
}

/// Sets a property of the host's own style only where it says something else, so a served host is not rewritten with what it already carries.
fn set_host_property(host: &web_sys::HtmlElement, name: &str, value: &str) {
    let style = host.style();
    if style.get_property_value(name).ok().as_deref() != Some(value) {
        let _ = style.set_property(name, value);
    }
}

/// Removes every child past `keep`, which is what a box that lost children leaves behind.
pub(crate) fn truncate(parent: &web_sys::Node, keep: u32) {
    while parent.child_nodes().length() > keep {
        let Some(extra) = parent.last_child() else {
            return;
        };
        let _ = parent.remove_child(&extra);
    }
}

/// Listens for the scroll a box performs on its own, and reports where it ended up.
///
/// The offset is read back rather than accumulated from deltas: the compositor may have applied several between two of these, and a rubber-band at the edge undoes part of what it applied. Where it *is* is the only thing that is true. Puts a box's own scroll where the widget is asking for it.
///
/// The offset travels the other way on almost every frame — the compositor scrolls, and `watch_scroll` reports where the content ended up. This is the other direction, and without it a widget had no way to move a box the compositor is holding: the scrollbar could not be dragged with a mouse, and a page navigated to opened wherever the last one had been left.
///
/// Only where the widget asks, and only for the one frame it asks in. Written every frame it would fight the scroll it is reporting — a fling is an offset the widget learns of a frame late, and answering with that stale value stops it dead.
fn settle_scroll(node: &web_sys::Element, scroll_to: Option<(f32, f32)>) {
    let Some((x, y)) = scroll_to else {
        return;
    };
    // Compared before writing: an assignment that changes nothing still costs a layout flush, and this runs while the frame is being built.
    if (node.scroll_left() as f32 - x).abs() >= 0.5 {
        node.set_scroll_left(x.round() as i32);
    }
    if (node.scroll_top() as f32 - y).abs() >= 0.5 {
        node.set_scroll_top(y.round() as i32);
    }
}

fn watch_scroll(node: &web_sys::Element, id: u64) -> Option<Closure<dyn FnMut(web_sys::Event)>> {
    let target = node.clone();
    let closure = Closure::<dyn FnMut(web_sys::Event)>::new(move |_: web_sys::Event| {
        report_scroll(&target, id);
    });
    // Passive: this only reports, and saying so lets the browser scroll without waiting to hear whether the listener wanted to prevent it — which is the whole reason a compositor scroll stays smooth.
    let options = web_sys::AddEventListenerOptions::new();
    options.set_passive(true);
    node.add_event_listener_with_callback_and_add_event_listener_options(
        "scroll",
        closure.as_ref().unchecked_ref(),
        &options,
    )
    .ok()?;
    Some(closure)
}

fn report_scroll(node: &web_sys::Element, id: u64) {
    platform_core::post_event(platform_core::Event::BoxScrolled {
        box_id: id,
        x: node.scroll_left() as f32,
        y: node.scroll_top() as f32,
    });
}

/// Reports where the document moves focus on its own — its own Tab order — so the app's focus follows it.
///
/// A box the document focuses becomes the app's focused box; Telar's own focus comes back through here too, and is answered as a move to where focus already is. Focus leaving the app for content beside it, or for the browser's own interface, leaves no box holding it.
///
/// So does focus landing inside the app on something that is no box: a link inside a paragraph, a scroll area the browser made focusable. The box Telar had focused would otherwise keep its ring and keep answering keys, and Enter on the link would press it too.
fn follow_focus(host: &web_sys::HtmlElement) -> Option<FocusFollower> {
    let own_host = host.clone();
    let into = Closure::<dyn FnMut(web_sys::Event)>::new(move |event: web_sys::Event| {
        let Some(element) = event
            .target()
            .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
        else {
            return;
        };
        match element
            .get_attribute(FOCUS_BOX_ATTRIBUTE)
            .and_then(|id| id.parse::<u64>().ok())
        {
            Some(box_id) => platform_core::post_event(platform_core::Event::BoxFocused { box_id }),
            None if !parks_telars_focus(&element, &own_host) => {
                platform_core::post_event(platform_core::Event::FocusLeftBoxes)
            }
            None => {}
        }
    });
    let watched = host.clone();
    let out = Closure::<dyn FnMut(web_sys::Event)>::new(move |event: web_sys::Event| {
        let next = event
            .dyn_ref::<web_sys::FocusEvent>()
            .and_then(|event| event.related_target());
        match next
            .as_ref()
            .and_then(|next| next.dyn_ref::<web_sys::Node>())
        {
            Some(next) if watched.contains(Some(next)) => {}
            Some(_) => platform_core::post_event(platform_core::Event::FocusLeftBoxes),
            // No next element is also a window losing focus, or a focused element being replaced; which one is only knowable once the move has settled.
            None => {
                let watched = watched.clone();
                let settled = Closure::once_into_js(move || {
                    if focus_went_to_the_browser(&watched) {
                        platform_core::post_event(platform_core::Event::FocusLeftBoxes);
                    }
                });
                if let Some(window) = web_sys::window() {
                    let _ = window.set_timeout_with_callback(settled.unchecked_ref());
                }
            }
        }
    });
    let options = web_sys::AddEventListenerOptions::new();
    options.set_passive(true);
    for (name, closure) in [("focusin", &into), ("focusout", &out)] {
        host.add_event_listener_with_callback_and_add_event_listener_options(
            name,
            closure.as_ref().unchecked_ref(),
            &options,
        )
        .ok()?;
    }
    Some(FocusFollower {
        host: host.clone(),
        into,
        out,
    })
}

/// Whether `element` is one of the two places Telar parks the document's focus itself: the host, while no box holds the keyboard, and the entry a field is typed through.
fn parks_telars_focus(element: &web_sys::Element, host: &web_sys::HtmlElement) -> bool {
    element.is_same_node(Some(host.as_ref()))
        || element.has_attribute(crate::entry::ENTRY_ATTRIBUTE)
}

/// Whether focus that left the host with nowhere to go went to the browser's own interface: the document lost it, and the element that had it no longer does. A window that only lost focus keeps its focused element; a replaced element leaves the document focused.
fn focus_went_to_the_browser(host: &web_sys::HtmlElement) -> bool {
    let Some(document) = host.owner_document() else {
        return false;
    };
    let still_inside = document
        .active_element()
        .is_some_and(|active| host.contains(Some(active.as_ref())));
    !document.has_focus().unwrap_or(true) && !still_inside
}

/// The focus listeners on the host, taken off when the reconcile goes: the host outlives it, and a listener left behind would call into a closure that no longer exists.
struct FocusFollower {
    host: web_sys::HtmlElement,
    into: Closure<dyn FnMut(web_sys::Event)>,
    out: Closure<dyn FnMut(web_sys::Event)>,
}

impl Drop for FocusFollower {
    fn drop(&mut self) {
        for (name, closure) in [("focusin", &self.into), ("focusout", &self.out)] {
            let _ = self
                .host
                .remove_event_listener_with_callback(name, closure.as_ref().unchecked_ref());
        }
    }
}
