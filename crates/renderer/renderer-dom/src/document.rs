//! One frame as the document it describes: which elements, in which order, saying what.
//!
//! Worked out here once and handed to two writers. In the browser the reconcile brings the live document in line with it, element by element; on a host a prerender writes it out as markup (`html.rs`). Both read the same [`Frame`], so a page written ahead of time and the page the app then draws are the same elements with the same attributes, and a client can adopt the first rather than build it again.
//!
//! The walk is pure except where a document has to be asked something: where the surface is in the viewport while the document scrolls, the address a bitmap is drawn from, and where a line of text sits on its baseline. A [`Surface`] answers those, the browser by measuring and a host by assuming a page at rest.
//!
//! A box is a box, but not everything a box paints is one. Three things arrive inside an element: its own background, which is CSS; child boxes, which the browser lays out; and paint that is neither — a caret, a selection band, a scrollbar. The last of those become positioned children, in the order they were drawn, so what covered what on a canvas covers the same thing here.
//!
//! And a frame paints at its own level too, outside every element: an application's shell fills the panel its rail stands on, and dims the page behind a drawer. That becomes a box inside the host, placed as it is drawn — see [`Walk::paint_at_root`].

use std::rc::Rc;
use std::sync::Arc;

use geometry_core::Rect;
use platform_core::Destination;
use platform_core::consumed_keys::{CONSUMED_KEYS_ATTRIBUTE, FOCUS_BOX_ATTRIBUTE};
use renderer_core::{
    BlendMode, Color, DrawCommand, Element, Focusable, ImageData, Picture, Role, TextStyle,
    ToggleKind,
};

use crate::paint;
use crate::runs::Run;
use crate::vector::Drawing;

/// Names the box an element stands for, on every element a box becomes: what a client taking over a prerendered page matches the elements it finds against.
pub const ID_ATTRIBUTE: &str = "data-telar-id";

/// Marks the element the app fills, so the reset below reaches its boxes and nothing else on the page.
pub(crate) const HOST_ATTRIBUTE: &str = "data-telar";
pub(crate) const RESET_ID: &str = "telar-reset";

/// Where a box was told to be, beside where the browser put it. See `reconcile.rs`.
pub(crate) const AUDIT_ATTRIBUTE: &str = "data-telar-rect";

/// What a document brings to an element that Telar never asked for: a button's border and its own font, a heading's margins, a link's colour and underline. A widget's style is the whole of what its box looks like, and the browser's idea of it is the difference between what layout computed and what the page shows — a button's 2px frame made every row of a list four pixels taller than the rect hit-testing reads.
///
/// One rule rather than a declaration per box per frame, and the base font is the one the measurer assumes, so a paragraph is drawn in the face it was measured in.
///
/// `color-scheme` is what dresses everything the browser draws itself and Telar cannot reach — the selection band, an autofill panel, the overlay scrollbar a nested document keeps. Declared here it follows the system, which is what an app that named no background of its own is doing too; one that named a colour overrides it from that colour (see [`Frame::background`]). `color` goes with it because the rule below makes every box inherit one: without it a box that draws no text of its own inherited the page's black, under a dark theme as much as a light one.
///
/// The scrollbars go too, and not for looks: a native one takes width out of the box it is in, layout never reserved it, and the sidebar came out fifteen pixels narrower than every rect hit-testing reads — with a horizontal scrollbar underneath for the fifteen pixels that no longer fitted. The scrolling stays the browser's; only the bar is Telar's, as it is on every other target.
///
/// The outline goes only where Telar draws a ring of its own: a focusable box (`data-telar-focus`). Whatever else the browser walks Tab through — a link inside a paragraph, a scroll area Firefox makes focusable — keeps the browser's ring, since nothing else would show where the keyboard is. Forced colours drop the shadow Telar's ring is painted with, so there every focused element takes the browser's ring back, the field entry included.
pub(crate) const RESET: &str = "[data-telar]{font:400 16px sans-serif;color-scheme:light dark;color:CanvasText}\
[data-telar] *{margin:0;border:0;padding:0;background:none;font:inherit;color:inherit;\
text-align:inherit;text-decoration:none;box-sizing:border-box;appearance:none;scrollbar-width:none;\
-webkit-appearance:none;outline:none}\
[data-telar] :focus-visible:not([data-telar-focus]){outline:revert}\
@media (forced-colors:active){[data-telar] :focus-visible{outline:revert!important}}\
[data-telar] *::-webkit-scrollbar{display:none}";

/// Host properties the document scroll overrides, and what they must be for it: a host that keeps a fixed height stops the page from growing, and one that declines touch gestures stops a finger from scrolling it, because a touch pans only when every box between it and the scroller allows it. A page scrolls down only, like every other `ScrollPage`, so content wider than it is clipped rather than handed to the document as a sideways scroll; `clip`, not `hidden`, because `hidden` would make the host a scroller and take the scroll away from the document.
pub(crate) const DOCUMENT_SCROLL_OVERRIDES: [(&str, &str); 3] = [
    ("height", "auto"),
    ("touch-action", "pan-x pan-y"),
    ("overflow-x", "clip"),
];

/// Text a drag across this box must not select, because the drag means something else there.
///
/// A document starts a selection under any drag that begins on selectable content, and sweeping one out of a margin and across a page is exactly what a person expects — so this is not for boxes at large. It is for the two kinds where a drag is already spoken for: a control, which is what a browser's own stylesheet says this about (`<button>`, `<input>`); and paint that is not a box at all — a scrollbar's thumb, a caret, a panel a shell fills behind its rail — which has nothing to select in the first place. Dragging the bar of a scroll area used to sweep a selection across everything it scrolled past.
const UNSELECTABLE: &str = "-webkit-user-select:none;user-select:none;";

const IDENTITY: [f32; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// What the walk has to ask of the document it is describing.
pub(crate) trait Surface {
    /// Box `id` is the surface's primary scroll this frame, which the document scrolls for it.
    fn hold_document_scroll(&mut self, id: u64);

    /// Where the surface's origin is in the viewport while the document scrolls the page; `None` while it does not, and a box placed against the surface is then placed inside the host.
    fn fixed_origin(&mut self) -> Option<(f32, f32)>;

    /// Where the host is in the viewport while the document does not scroll the page, for a layer fixed over the surface, which is fixed against the viewport either way.
    fn host_origin(&mut self) -> (f32, f32);

    /// The address a bitmap drawn inside a drawing is loaded from, or `None` for one this document cannot show.
    fn image_href(&mut self, data: &ImageData) -> Option<Rc<str>>;

    /// How far below the top of its line box a line in `style` sits on its baseline.
    fn baseline(&mut self, style: &TextStyle) -> f32;
}

/// One frame, as the elements the host holds.
pub(crate) struct Frame {
    /// The surface's own colour, which the host carries. `None` for a surface that named none, and for one fully transparent, which is an application asking to see the page through it.
    pub background: Option<Color>,
    /// Whether a box blended directly against the host, which then has to start a stacking context of its own.
    pub isolate_host: bool,
    /// The box that took the document scroll this frame.
    pub primary: Option<u64>,
    /// The host's children, in order: the layout roots and the paint the frame carries at its own level.
    pub children: Vec<Node>,
}

/// A child of the host.
pub(crate) enum Node {
    Box(Box<BoxNode>),
    Paint(PaintNode),
}

/// One box, as the element it becomes.
pub(crate) struct BoxNode {
    pub id: u64,
    /// The element a role *is*; a box whose role changes needs a different element, not a new attribute.
    pub tag: &'static str,
    pub role: Role,
    /// Whether the box scrolls its own content, as an element with an overflow of its own.
    pub scrolls: bool,
    /// Whether the box is the surface's primary scroll, which the document scrolls for it.
    pub primary: bool,
    /// Everything the `style` attribute says, layout and paint together.
    pub style: String,
    pub described: Described,
    /// The picture an `<img>` shows, and the width it is laid out at.
    pub picture: Option<(Arc<Picture>, f32)>,
    pub scroll_to: Option<(f32, f32)>,
    /// The rect layout computed, carried only when the page asked to compare the two.
    pub audit: Option<Rect>,
    pub content: Content,
}

/// What an element holds.
pub(crate) enum Content {
    /// Child boxes, then the paint inside the box that is not a box, in the order it was drawn: a scroll area's bars are drawn over the content they scroll.
    Children {
        boxes: Vec<BoxNode>,
        pieces: Vec<PaintNode>,
    },
    /// A single run of text, which is the element's own text rather than something inside it — what lets it be selected, found and read as part of the document.
    Text {
        text: String,
        /// The paragraph cut at its spans, when it has any: written as inline elements rather than as one string.
        runs: Option<Vec<Run>>,
    },
    /// The markup of a drawing, whose boxes are part of its picture rather than elements of the page.
    Drawing(String),
}

/// Paint that is not a box — a caret, a selection band, a scrollbar's thumb, a panel a shell fills — placed where it was drawn.
pub(crate) struct PaintNode {
    pub style: String,
    pub text: String,
    pub runs: Option<Vec<Run>>,
}

/// What a box is, as the attributes that say so.
#[derive(Clone, Default, PartialEq)]
pub(crate) struct Described {
    pub role: Option<&'static str>,
    pub label: Option<String>,
    pub link: Option<String>,
    // Whether the link is an external one, which a page must not be able to reach back from.
    pub external: bool,
    pub opens_beside: bool,
    pub lang: Option<String>,
    pub anchor: Option<String>,
    pub hidden: bool,
    /// A control's on/off state and what its role makes it, which decides the attribute that carries it.
    pub toggled: Option<(ToggleKind, bool)>,
    pub disabled: bool,
    /// Part of the record even though it writes no attribute: a box that has just become the focused one is a box the reconcile has to act on, and comparing without it made the acting unreachable.
    pub focused: bool,
    pub control: bool,
    pub focusable: Option<Focusable>,
    /// Whether the element is an `<img>`, which is named by its `alt` rather than a label.
    pub picture: bool,
}

impl Described {
    /// Every attribute this speaks for, in the order an element is given them, with `None` for one the element has to be without. Removing matters as much as setting: a box that stops being a link keeps sending the reader somewhere until the `href` goes.
    pub(crate) fn attributes(&self, id: u64) -> Vec<(&'static str, Option<String>)> {
        // An `<img>` is named by its `alt`, and an empty one is how it says it is decoration.
        let (alt, label) = match self.picture {
            true => (Some(self.label.clone().unwrap_or_default()), None),
            false => (None, self.label.clone()),
        };
        // The browser walks Tab through the boxes Telar says are stops, in document order, which is the order Telar registers them in; everything else focusable takes focus only when Telar gives it.
        let tabindex = match self.focusable {
            Some(focusable) if focusable.tab_stop => Some("0"),
            Some(_) => Some("-1"),
            None => self.control.then_some("-1"),
        };
        let keys = self
            .focusable
            .map(|focusable| focusable.consumes.to_names())
            .filter(|names| !names.is_empty());
        let flag = |on: bool| on.then(|| "true".to_string());
        let mut attributes = vec![("role", self.role.map(str::to_string))];
        if self.picture {
            attributes.push(("alt", alt));
        } else {
            attributes.push(("aria-label", label));
        }
        attributes.extend([
            ("href", self.link.clone()),
            ("target", self.opens_beside.then(|| "_blank".to_string())),
            ("rel", self.external.then(|| "noopener".to_string())),
            ("lang", self.lang.clone()),
            ("id", self.anchor.clone()),
            ("aria-hidden", flag(self.hidden)),
            ("aria-checked", self.state_of(ToggleKind::Checked)),
            ("aria-pressed", self.state_of(ToggleKind::Pressed)),
            ("aria-selected", self.state_of(ToggleKind::Selected)),
            ("aria-expanded", self.state_of(ToggleKind::Expanded)),
            ("aria-disabled", flag(self.disabled)),
            ("tabindex", tabindex.map(str::to_string)),
            (CONSUMED_KEYS_ATTRIBUTE, keys),
            (FOCUS_BOX_ATTRIBUTE, self.focusable.map(|_| id.to_string())),
        ]);
        attributes
    }

    /// The value of the state attribute for `kind`: written only on the role that state belongs to, so a switch never carries `aria-pressed` and a toggle button never `aria-checked`.
    fn state_of(&self, kind: ToggleKind) -> Option<String> {
        self.toggled
            .filter(|(carried, _)| *carried == kind)
            .map(|(_, on)| on.to_string())
    }
}

/// Every attribute of a linked picture's `<img>` but its `alt`, in the order they are written: `srcset` before `src`, because a browser that sees the address first starts fetching the full-size copy before it has been told there are smaller ones. `None` for one the element has to be without.
pub(crate) fn picture_attributes(
    picture: &Picture,
    width: f32,
) -> Vec<(&'static str, Option<String>)> {
    let source = &picture.source;
    let (srcset, sizes) = match source.variants.is_empty() {
        true => (None, None),
        false => (
            Some(srcset(picture)),
            Some(format!("{}px", picture_slot(width))),
        ),
    };
    vec![
        ("srcset", srcset),
        ("sizes", sizes),
        ("width", Some(picture.width.to_string())),
        ("height", Some(picture.height.to_string())),
        ("decoding", Some("async".to_string())),
        ("loading", (!picture.priority).then(|| "lazy".to_string())),
        (
            "fetchpriority",
            picture.priority.then(|| "high".to_string()),
        ),
        ("src", Some(platform_core::asset_url(&source.url))),
    ]
}

/// The width a picture is chosen for, in whole CSS pixels.
pub(crate) fn picture_slot(width: f32) -> u32 {
    width.max(1.0).ceil() as u32
}

/// Every copy with the width it was made at, the full-size one last, which is what lets the browser weigh them against the box and the screen's density.
pub(crate) fn srcset(picture: &Picture) -> String {
    picture
        .source
        .variants
        .iter()
        .map(|(width, url)| (*width, url.as_ref()))
        .chain(std::iter::once((
            picture.width,
            picture.source.url.as_ref(),
        )))
        .map(|(width, url)| format!("{} {width}w", platform_core::asset_url(url)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Works out the document `commands` describe. `audit` carries each box's computed rect along, for a page that compares the two.
pub(crate) fn describe_frame(
    commands: &[DrawCommand],
    clear: Option<Color>,
    surface: &mut dyn Surface,
    audit: bool,
) -> Frame {
    let mut walk = Walk {
        surface,
        audit,
        open: vec![Open::root()],
        primary: None,
        isolate_host: false,
        roots: Vec::new(),
        layers: Vec::new(),
    };
    // Boxes inside a drawing are part of its picture, not elements of the page: it places them itself.
    let mut boxes_in_drawing = 0usize;
    for command in commands {
        let in_drawing = walk.open.last().is_some_and(|open| open.drawing.is_some());
        match command {
            DrawCommand::PushElement { .. } if in_drawing => {
                boxes_in_drawing += 1;
                walk.paint(command);
            }
            DrawCommand::PopElement if boxes_in_drawing > 0 => {
                boxes_in_drawing -= 1;
                walk.paint(command);
            }
            DrawCommand::PushElement { element } => walk.push(element),
            DrawCommand::PopElement => walk.pop(),
            other => walk.paint(other),
        }
    }
    let mut roots = walk.roots;
    // The outermost layer closed last, and a layer declared inside it can only find its place once that one is in the tree.
    for (place, layer) in walk.layers.into_iter().rev() {
        if let Err(layer) = put_in_place(&mut roots, place, layer) {
            roots.push(Node::Box(layer));
        }
    }
    Frame {
        background: clear.filter(|color| color.a > 0.0),
        isolate_host: walk.isolate_host,
        primary: walk.primary,
        children: roots,
    }
}

/// Puts a layer fixed over the surface where the box holding its place stands, handing it back when no box does.
///
/// Lifted there over every box of the page, sticky ones included, which are positioned too and come later in the document; and the layout root it sits in becomes a stacking context of its own, so the lift stays inside it and an overlay placed after that root still covers the layer.
fn put_in_place(
    roots: &mut [Node],
    place: u64,
    mut layer: Box<BoxNode>,
) -> Result<(), Box<BoxNode>> {
    for root in roots.iter_mut() {
        let Node::Box(root) = root else {
            continue;
        };
        if root.id == place {
            *root = layer;
            return Ok(());
        }
        let Some(holder) = box_within(root, place) else {
            continue;
        };
        paint::declare(&mut layer.style, "z-index", "1");
        *holder = *layer;
        if !root.style.contains("isolation:") {
            paint::declare(&mut root.style, "isolation", "isolate");
        }
        return Ok(());
    }
    Err(layer)
}

/// The box `id` somewhere beneath `node`.
fn box_within(node: &mut BoxNode, id: u64) -> Option<&mut BoxNode> {
    let Content::Children { boxes, .. } = &mut node.content else {
        return None;
    };
    for child in boxes.iter_mut() {
        if child.id == id {
            return Some(child);
        }
        if let Some(found) = box_within(child, id) {
            return Some(found);
        }
    }
    None
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
        runs: Option<Vec<Run>>,
        /// Whether its style takes the element's background, which keeps it out of a box that paints one.
        claims_background: bool,
    },
}

/// What is being assembled while the walk is inside one element.
struct Open {
    /// The box as far as its push could say; `None` for the host.
    node: Option<BoxNode>,
    /// Where layout put the box, so paint that *is* the box can be told from paint that is inside it.
    box_rect: Rect,
    /// Set for a box whose content is drawn rather than laid out; everything inside it goes here.
    drawing: Option<Drawing>,
    style: String,
    /// Whether the element's own background has been taken, so a box painted twice keeps the first.
    painted: bool,
    boxes: Vec<BoxNode>,
    pieces: Vec<Painted>,
    /// A transform whose subject is not yet known: the box itself if its own paint turns up inside, and the boxes it wraps otherwise.
    moved: Option<[f32; 6]>,
    /// Whether this box scrolls its own content, which is what makes its clip an overflow rather than a cut.
    scrolls: bool,
    /// Whether this box is the surface's primary scroll, which the document scrolls for it: it neither cuts nor scrolls what it holds.
    primary: bool,
    /// Whether the box is an `<img>`, whose picture the browser draws: what the widget painted for every other target is not wanted here.
    picture: bool,
    /// Set on a layer fixed over the surface: the box holding its place where it was declared, which the element takes once the frame is walked.
    fixed_in_place_of: Option<u64>,
}

impl Open {
    fn root() -> Self {
        Self {
            node: None,
            box_rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            drawing: None,
            style: String::new(),
            // The host takes no paint of its own: the page chose that element's size and the application named its background in `clear_color`, which the host carries. What the frame draws at this level becomes a box inside it instead.
            painted: true,
            boxes: Vec::new(),
            pieces: Vec::new(),
            moved: None,
            scrolls: false,
            primary: false,
            picture: false,
            fixed_in_place_of: None,
        }
    }

    fn is_root(&self) -> bool {
        self.node.is_none()
    }
}

struct Walk<'a> {
    surface: &'a mut dyn Surface,
    audit: bool,
    open: Vec<Open>,
    primary: Option<u64>,
    isolate_host: bool,
    roots: Vec<Node>,
    /// The layers fixed over the surface, closed and waiting for the boxes that hold their places, in the order they closed.
    layers: Vec<(u64, Box<BoxNode>)>,
}

impl Walk<'_> {
    /// Fixes a layer against the viewport, over the whole surface, wherever its element ends up. The layer spans the surface but takes no pointer of its own: its boxes take it back, and everywhere else a press reaches the page under it.
    fn fix_against_viewport(&mut self, style: &mut String, rect: Rect) {
        let (x, y) = match self.surface.fixed_origin() {
            Some(origin) => origin,
            None => self.surface.host_origin(),
        };
        paint::declare(style, "position", "fixed");
        paint::declare(style, "left", &paint::px(rect.x + x));
        paint::declare(style, "top", &paint::px(rect.y + y));
        paint::declare(style, "width", &paint::px(rect.width));
        paint::declare(style, "height", &paint::px(rect.height));
        paint::declare(style, "pointer-events", "none");
    }

    /// Places a box against the surface: inside the host, or against the viewport while the document scrolls the page, so what stands over the page stays put as it scrolls.
    fn place_on_surface(&mut self, style: &mut String, rect: Rect) {
        let (position, x, y) = match self.surface.fixed_origin() {
            Some((x, y)) => ("fixed", rect.x + x, rect.y + y),
            None => ("absolute", rect.x, rect.y),
        };
        paint::declare(style, "position", position);
        paint::declare(style, "left", &paint::px(x));
        paint::declare(style, "top", &paint::px(y));
    }

    /// A box for paint the frame carries at its own top level.
    ///
    /// A widget may draw where there is no element for it to be the background of: an application's shell paints the panel its rail stands on before it draws the rail, and dims the page behind a drawer. The host cannot take it — the page chose that element's size and the application named its background in `clear_color` — so it becomes a box of its own inside it. Dropped, as it was, the rail stood on the page's own colour and every pill in it that had been invisible against its panel was suddenly a shape.
    ///
    /// Put in place as it is drawn, and not collected the way paint *inside* an element is. There the pieces go after the boxes because that is what they are — a scroll area's bar is drawn over the content it scrolls. Here the order is the frame's own: a panel drawn before the rail belongs under it, and holding it back would have laid it over the thing it stands behind.
    fn paint_at_root(&mut self, rect: Rect, painted: &str, text: &str) {
        let mut style = String::new();
        self.place_on_surface(&mut style, rect);
        paint::declare(&mut style, "width", &paint::px(rect.width.max(0.0)));
        paint::declare(&mut style, "height", &paint::px(rect.height.max(0.0)));
        // This answers no pointer: the boxes do, and a pane of paint across them would swallow every press meant for what is underneath.
        paint::declare(&mut style, "pointer-events", "none");
        style.push_str(UNSELECTABLE);
        if let Some(matrix) = self.open.last().and_then(|root| root.moved) {
            paint::declare(&mut style, "transform-origin", "0 0");
            paint::declare(
                &mut style,
                "transform",
                &paint::matrix(matrix, rect.x, rect.y),
            );
        }
        style.push_str(painted);
        self.roots.push(Node::Paint(PaintNode {
            style,
            text: text.to_string(),
            runs: None,
        }));
    }

    /// Confines a blended box's `mix-blend-mode` to its own siblings. Left alone, the blend reaches past its parent to whatever stacking context is nearest — for an otherwise plain tree, the page itself — so a texture meant to multiply against its neighbour would also ghost into content several levels up. `isolation: isolate` on the parent starts a stacking context there, which is what confines the backdrop a blended child sees to that parent's own children.
    fn isolate_parent(&mut self) {
        match self.open.len() {
            0 | 1 => {}
            // The blended box is itself a layout root, so its backdrop is the page behind the host; isolating the host confines it to what the app itself drew.
            2 => self.isolate_host = true,
            len => {
                let parent = &mut self.open[len - 2];
                if !parent.style.contains("isolation:") {
                    paint::declare(&mut parent.style, "isolation", "isolate");
                }
            }
        }
    }

    /// Everything that is not an element boundary: what the open box paints.
    fn paint(&mut self, command: &DrawCommand) {
        if self
            .open
            .last()
            .is_some_and(|open| open.is_root() && open.drawing.is_none())
        {
            match command {
                DrawCommand::Rect { rect, style } => {
                    let mut css = String::new();
                    paint::rect_style(style, *rect, &mut css);
                    self.paint_at_root(*rect, &css, "");
                    return;
                }
                DrawCommand::Text {
                    text, rect, style, ..
                } => {
                    let mut css = String::new();
                    paint::text_style(style, &mut css);
                    self.paint_at_root(*rect, &css, text);
                    return;
                }
                _ => {}
            }
        }
        let Some(open) = self.open.last_mut() else {
            return;
        };
        if let Some(drawing) = open.drawing.as_mut() {
            draw(drawing, command, self.surface);
            return;
        }
        match command {
            DrawCommand::Rect { rect, style } => {
                if open.painted {
                    return;
                }
                // The box's own background is the one that is the box. Anything else is paint the widget put inside it, and folding that into the background would spread one small mark over the whole element.
                if is_own_box(*rect, open.box_rect) {
                    open.painted = true;
                    // A matrix this box's own paint sits inside is the box's own transform, and now it is known to be: the boxes it also wraps are moved by moving the box.
                    if let Some(matrix) = open.moved.take() {
                        let at = open.box_rect;
                        paint::declare(&mut open.style, "transform-origin", "0 0");
                        paint::declare(
                            &mut open.style,
                            "transform",
                            &paint::matrix(matrix, at.x, at.y),
                        );
                    }
                    paint::rect_style(style, *rect, &mut open.style);
                    return;
                }
                let mut css = String::new();
                paint::rect_style(style, *rect, &mut css);
                open.pieces.push(Painted::Rect {
                    rect: *rect,
                    style: css,
                });
            }
            DrawCommand::Text {
                text,
                rect,
                style,
                spans,
            } => {
                let mut css = String::new();
                paint::text_style(style, &mut css);
                open.pieces.push(Painted::Text {
                    rect: *rect,
                    style: css,
                    text: text.to_string(),
                    runs: spans
                        .as_deref()
                        .filter(|spans| !spans.is_empty())
                        .map(|spans| crate::runs::runs_of(text, spans)),
                    claims_background: paint::text_claims_background(style),
                });
            }
            DrawCommand::PushLayer { opacity, blend, .. } => {
                if *opacity < 1.0 {
                    paint::declare(&mut open.style, "opacity", &paint::round(*opacity));
                }
                if *blend != BlendMode::Normal {
                    paint::declare(&mut open.style, "mix-blend-mode", blend.css_name());
                    self.isolate_parent();
                }
            }
            DrawCommand::PushClip { .. } if open.primary => {}
            DrawCommand::PushClip { radius, .. } => {
                // A scroll area clips the same way, and the difference is the whole point: `hidden` cuts what does not fit, `auto` lets the compositor move it — and with it find-in-page, the keyboard, `scrollIntoView` and every anchor, none of which a transform can give back.
                // `clip` rather than `hidden` for a cut: `hidden` makes the box a scroll container, and a sticky box inside it would stick to that box, which never scrolls, instead of to the scroll viewport Telar sticks it to.
                let overflow = if open.scrolls { "auto" } else { "clip" };
                paint::declare(&mut open.style, "overflow", overflow);
                if !radius.is_zero() {
                    paint::declare(
                        &mut open.style,
                        "border-radius",
                        &paint::px(radius.top_left),
                    );
                }
            }
            // A matrix moves whatever it wraps, and which that is only becomes clear inside it. A widget that transforms itself draws its own box in there; a scroll area wraps its content and nothing else.
            DrawCommand::PushMatrix { matrix } => {
                if *matrix != IDENTITY {
                    open.moved = Some(*matrix);
                }
            }
            // Artwork reaches a document as an SVG, so one that arrives in a box means a widget drew geometry without saying its box was a drawing.
            DrawCommand::Image { .. } | DrawCommand::Path { .. } | DrawCommand::Line { .. } => {
                if open.picture {
                    return;
                }
                tracing::debug!("a box painted geometry it did not declare itself a drawing for");
            }
            DrawCommand::PopMatrix => open.moved = None,
            DrawCommand::PopClip | DrawCommand::PopLayer => {}
            DrawCommand::PushElement { .. } | DrawCommand::PopElement => {}
        }
    }

    fn push(&mut self, element: &Element) {
        let tag = match element.picture {
            Some(_) => "img",
            None => tag_of(&element.semantics.role),
        };
        let fixed_in_place_of = element.fixed_in_place_of.map(|place| place.0);
        let primary = element.primary_scroll && self.open.len() == 1 && self.primary.is_none();
        if primary {
            self.primary = Some(element.id.0);
            self.surface.hold_document_scroll(element.id.0);
        }
        let scrolls = element.semantics.role == Role::ScrollArea && !primary;
        let drawing = matches!(element.semantics.role, Role::Drawing) && element.picture.is_none();
        let mut style = String::new();
        if let Some(picture) = &element.picture {
            paint::declare(&mut style, "display", "block");
            paint::declare(&mut style, "object-fit", picture.fit);
        }
        if drawing {
            // An `<svg>` is inline by default, reserving a descender's worth of space under it that the box it stands in never asked for. The declarations follow, so a box that wants another display still gets it.
            paint::declare(&mut style, "display", "block");
        }
        if element.semantics.role.is_control() {
            style.push_str(UNSELECTABLE);
        }
        style.push_str(&element.layout);
        // A box whose parent is the host is a layout root: the application computed and placed it itself, so there is no parent expressing where it goes and the declarations alone would stack them. One of the two places the computed rect is used instead of what the box asked for; a layer fixed over the surface, placed against the viewport wherever its element ends up, is the other.
        // The primary scroll is the exception: it stays in the flow and grows with its content, which is what makes the document tall enough to scroll, and it is at least the surface's height so a short page still fills it.
        if fixed_in_place_of.is_some() {
            self.fix_against_viewport(&mut style, element.rect);
        } else if primary {
            paint::declare(&mut style, "min-height", &paint::px(element.rect.height));
        } else if self.open.len() == 1 {
            let rect = element.rect;
            self.place_on_surface(&mut style, rect);
            paint::declare(&mut style, "width", &paint::px(rect.width));
            paint::declare(&mut style, "height", &paint::px(rect.height));
        }
        if scrolls {
            // The host declines touch gestures so a drag inside the app does not pan the page; a box that scrolls has to take them back, or a finger moves nothing at all.
            paint::declare(&mut style, "touch-action", "pan-x pan-y");
            // What is scrolled to the end is the end. Without this the page behind takes over and the app slides away under the finger.
            paint::declare(&mut style, "overscroll-behavior", "contain");
        }
        if self
            .open
            .last()
            .is_some_and(|parent| parent.fixed_in_place_of.is_some())
        {
            paint::declare(&mut style, "pointer-events", "auto");
        }
        if element.semantics.click_through {
            paint::declare(&mut style, "pointer-events", "none");
        }
        // A transform its parent is still holding wraps this box rather than the parent: a scroll area moves its content, and moving the viewport instead takes the panel off the page.
        if let Some(matrix) = self.open.last().and_then(|parent| parent.moved) {
            let at = element.rect;
            paint::declare(&mut style, "transform-origin", "0 0");
            paint::declare(&mut style, "transform", &paint::matrix(matrix, at.x, at.y));
        }
        let node = BoxNode {
            id: element.id.0,
            tag,
            role: element.semantics.role,
            scrolls,
            primary,
            style: String::new(),
            described: describe(element, tag),
            picture: element
                .picture
                .as_ref()
                .map(|picture| (Arc::clone(picture), element.rect.width)),
            scroll_to: element.scroll_to,
            audit: self.audit.then_some(element.rect),
            content: Content::Children {
                boxes: Vec::new(),
                pieces: Vec::new(),
            },
        };
        self.open.push(Open {
            node: Some(node),
            box_rect: element.rect,
            drawing: drawing.then(|| Drawing::at(element.id.0, (element.rect.x, element.rect.y))),
            style,
            painted: false,
            boxes: Vec::new(),
            pieces: Vec::new(),
            moved: None,
            scrolls,
            primary,
            picture: element.picture.is_some(),
            fixed_in_place_of,
        });
    }

    fn pop(&mut self) {
        if self.open.len() <= 1 {
            return;
        }
        let Some(mut open) = self.open.pop() else {
            return;
        };
        let Some(mut node) = open.node.take() else {
            return;
        };
        // A single run of text is the box's own label, not something inside it, so it becomes the element's text and style — which is what lets it be selected, found and read as part of the document. Unless the text needs the background to itself: glyphs filled with a gradient are a background clipped to their shape.
        let inline_text = open.boxes.is_empty()
            && open.pieces.len() == 1
            && matches!(
                open.pieces[0],
                Painted::Text {
                    claims_background: false,
                    ..
                }
            );
        if inline_text && let Painted::Text { style, .. } = &open.pieces[0] {
            open.style.push_str(style);
        } else if !open.pieces.is_empty() && !open.style.contains("position:") {
            // Paint placed inside a box is placed against that box. Without this it is placed against whatever the nearest positioned ancestor happens to be, and a field's own text went to the corner of the page.
            paint::declare(&mut open.style, "position", "relative");
        }
        let fixed_in_place_of = open.fixed_in_place_of;
        node.style = open.style;
        node.content = if let Some(drawing) = open.drawing {
            Content::Drawing(drawing.finish())
        } else if inline_text {
            let Some(Painted::Text { text, runs, .. }) = open.pieces.pop() else {
                unreachable!("inline_text is exactly this shape")
            };
            Content::Text { text, runs }
        } else {
            Content::Children {
                boxes: open.boxes,
                pieces: open.pieces.into_iter().map(piece).collect(),
            }
        };
        match fixed_in_place_of {
            Some(place) => self.layers.push((place, Box::new(node))),
            None => self.place(node),
        }
    }

    /// Puts a closed box where the frame says it belongs: after the boxes already placed in the element being assembled, or the host's next child.
    fn place(&mut self, node: BoxNode) {
        let Some(parent) = self.open.last_mut() else {
            return;
        };
        // A drawing owns everything inside it as markup, so a box placed in one would be written over by the next frame that changes the picture.
        if parent.drawing.is_some() {
            return;
        }
        if parent.is_root() {
            self.roots.push(Node::Box(Box::new(node)));
        } else {
            parent.boxes.push(node);
        }
    }
}

/// A piece as the positioned element that stands for it.
fn piece(painted: Painted) -> PaintNode {
    let (rect, css, text, runs) = match painted {
        Painted::Rect { rect, style } => (rect, style, String::new(), None),
        Painted::Text {
            rect,
            style,
            text,
            runs,
            ..
        } => (rect, style, text, runs),
    };
    let mut style = String::new();
    paint::declare(&mut style, "position", "absolute");
    paint::declare(&mut style, "left", &paint::px(rect.x));
    paint::declare(&mut style, "top", &paint::px(rect.y));
    paint::declare(&mut style, "width", &paint::px(rect.width.max(0.0)));
    paint::declare(&mut style, "height", &paint::px(rect.height.max(0.0)));
    style.push_str(UNSELECTABLE);
    style.push_str(&css);
    PaintNode { style, text, runs }
}

/// Says what the box is, in whatever way the element it became does not already say it.
///
/// A `<nav>` needs no `role="navigation"` — it *is* one, and duplicating it is noise a reader has to step over. Only the roles with no element of their own carry the attribute.
fn describe(element: &Element, tag: &'static str) -> Described {
    let semantics = &element.semantics;
    let label = semantics.label.as_deref();
    let role = match (tag == "div" || tag == "svg")
        .then(|| aria_role(semantics.role))
        .flatten()
    {
        // A plain `div` may not carry a name, and a browser drops one it does; `group` is the generic role a name is allowed on.
        None if tag == "div" && label.is_some() => Some("group"),
        role => role,
    };
    // Artwork nobody named is decoration, and a graphic with no accessible name is noise to read out.
    let picture = tag == "img";
    let hidden =
        semantics.hidden || (semantics.role == Role::Drawing && label.is_none() && !picture);
    // An `<a>` without an `href` is no link, which is what a disabled one has to be.
    let link = semantics.link.as_ref().filter(|_| !semantics.disabled);
    Described {
        role,
        label: label.map(str::to_string),
        link: link.map(platform_core::address_of),
        external: matches!(link, Some(Destination::External(_))),
        opens_beside: matches!(link, Some(Destination::External(uri)) if uri.is_web()),
        lang: semantics.lang.as_deref().map(str::to_string),
        anchor: semantics.anchor.as_deref().map(str::to_string),
        hidden,
        toggled: semantics
            .toggled
            .and_then(|on| Some((semantics.role.toggle_kind()?, on))),
        disabled: semantics.disabled,
        focused: semantics.focused,
        control: semantics.role.is_control(),
        focusable: semantics.focusable,
        picture,
    }
}

/// Whether a painted rect is the box it was painted in, in either of the two ways a widget can say so: a box that draws its own frame knows where it is, and a leaf that draws inside itself starts at its corner.
fn is_own_box(rect: Rect, box_rect: Rect) -> bool {
    let same = |a: f32, b: f32| (a - b).abs() < 0.01;
    same(rect.width, box_rect.width)
        && same(rect.height, box_rect.height)
        && ((same(rect.x, box_rect.x) && same(rect.y, box_rect.y))
            || (same(rect.x, 0.0) && same(rect.y, 0.0)))
}

/// What one command adds to the picture an element is drawing.
fn draw(drawing: &mut Drawing, command: &DrawCommand, surface: &mut dyn Surface) {
    match command {
        DrawCommand::Rect { rect, style } => drawing.rect(*rect, style),
        DrawCommand::Text {
            text, rect, style, ..
        } if drawing.in_mask() => {
            let baseline = surface.baseline(style);
            drawing.mask_text(text, *rect, style, baseline)
        }
        DrawCommand::Text {
            text, rect, style, ..
        } => drawing.text(text, *rect, style),
        DrawCommand::Path { data, style } => drawing.path(data, style),
        DrawCommand::Line { p1, p2, style } => drawing.line(*p1, *p2, style),
        DrawCommand::Image {
            data,
            rect,
            raster,
            fill,
        } => {
            let href = surface.image_href(data);
            drawing.image(
                href.as_deref(),
                (data.width, data.height),
                *rect,
                *raster,
                *fill,
            );
        }
        DrawCommand::PushClip { rect, radius } => drawing.open_clip(*rect, *radius),
        DrawCommand::PushMatrix { matrix } => drawing.open_matrix(*matrix),
        DrawCommand::PushLayer {
            opacity,
            blend,
            mask,
            ..
        } => drawing.open_layer(*opacity, *blend, *mask),
        DrawCommand::PopClip | DrawCommand::PopMatrix | DrawCommand::PopLayer => {
            drawing.close_group()
        }
        DrawCommand::PushElement { element } => drawing.open_box((element.rect.x, element.rect.y)),
        DrawCommand::PopElement => drawing.close_group(),
    }
}

/// The element a role *is*.
///
/// A `div` is not a role that failed: it is one the document has no element for, and [`aria_role`] then says in an attribute what the tag could not. Preferring the element where there is one is not decoration — an element carries the meaning to a reader, to a search index and to a stylesheet, where an attribute reaches only the first.
pub(crate) fn tag_of(role: &Role) -> &'static str {
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
/// `None` where the role *is* the element, and where there is nothing worth announcing: a plain group is a `div`, and `role="group"` on every box in the tree is a reader reading out the scaffolding.
pub(crate) fn aria_role(role: Role) -> Option<&'static str> {
    match role {
        Role::Group => None,
        // A picture with a name; without one it is hidden instead, which `describe` decides.
        Role::Drawing => Some("img"),
        // A scroll area is a region a reader can be told about, but `scrollarea` is not an ARIA role and a browser would ignore it.
        Role::ScrollArea => None,
        // `ul` and `li` are a pair with a content model and nothing here can promise an author marked both. The ARIA roles are announced the same and are valid anywhere.
        Role::List => Some("list"),
        Role::ListItem => Some("listitem"),
        other => Some(other.as_str()),
    }
}

#[cfg(test)]
#[path = "document_test.rs"]
mod tests;
