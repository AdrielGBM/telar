//! What a drawn box *is*: which box it is across frames, and, for a backend that reconciles elements rather than rasterising pixels, what it means.
//!
//! A raster backend needs only the id: it is handed rects that are already where they belong, and uses the id to tell which box is which when it diffs a frame against the last. A backend whose output is a document needs that too, so it can move an element rather than rebuild it — and needs to know what a box *means*, so a button is a `<button>` and not a `<div>` that happens to be clickable.

use geometry_core::{Insets, Rect};

/// What a box *is* and what to call it. Shared with the platform layer rather than defined here: the desktop announcing a checkbox and a document drawing one are describing the same box, and two vocabularies for that is how they came to disagree.
pub use semantics_core::{
    Annotation, ConsumedKeys, CurrentKind, Destination, Focusable, Role, Semantics, ToggleKind,
};

/// Identifies one box across frames.
///
/// The layout node's own id: it is created with the widget, lives as long as it, and is already the thing every other layer uses to name that box. Minting a second identity would only be one more thing to keep in step.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct ElementId(pub u64);

/// One box in a frame: what it is, what it was asked for, and what to call it.
///
/// Carries its own layout rather than leaving a document backend to look it up. Two things follow. The backend becomes a pure function of the command stream — it needs no access to the layout engine, and can therefore be tested against a stream built on a machine with no browser. And the string is built where the style is already known, once per re-render of the widget that owns it, rather than once per frame.
#[derive(Clone, PartialEq, Debug)]
pub struct Element {
    pub id: ElementId,
    pub semantics: Semantics,
    /// The box's layout, as CSS declarations — `display:flex;gap:8px;` and so on. Empty where the target does not want them, which is every target that positions the box itself.
    pub layout: Box<str>,
    /// Where layout put the box, in the surface's own coordinates.
    ///
    /// Carried alongside the declarations, not instead of them, because the two answer different questions. A box inside another box is placed by its parent, and the declarations are what the parent needs. A box that *is* a layout root — an application that computes several and places them itself — has no parent to place it, and a document told only what it asked for would stack them.
    pub rect: Rect,
    /// Where a box that scrolls its own content is being *asked* to put it.
    ///
    /// The same shape of thing as [`Semantics::focused`], and for the same reason: a target that draws the content at the offset needs no telling, because there the offset is the whole of it. One that hands the box to a document does — there the compositor holds the content and reports where it moved it, so a widget that moves the offset itself (a bar dragged with the mouse, a jump to the top of a page just navigated to) is only stating a wish that the next report overrules. Said here, a backend can act on it.
    ///
    /// Not part of [`Semantics`], which is what the box *means* and is compared and hashed as such. This is a fact about one frame, like [`rect`](Self::rect) beside it, and it is `None` on almost every one: only the frame in which the widget asks carries it.
    pub scroll_to: Option<(f32, f32)>,
    /// Whether this box is the surface's primary scroll: the one scroll that stands for the whole page. A backend with a scroll of its own for the page maps this box onto it; every other backend treats it as any other box that scrolls.
    pub primary_scroll: bool,
    /// How far short of each edge of its view a box that scrolls stops what it brings into view, because something is drawn over that strip: a bar fixed over the page. A document says it as the scroller's `scroll-padding`, so the browser's own scrolling into view (a fragment, focus, paging with the keyboard) stops there too; a target that scrolls by drawing has already applied it to every reveal.
    pub arrival_margin: Insets,
    /// Set when the box is a picture at an address and nothing more: a document shows it as an `<img>`, which fetches, decodes and chooses a size by itself. See [`Picture`].
    pub picture: Option<std::sync::Arc<Picture>>,
    /// Set on the box a layer fixed over the surface is made of, naming the box that holds its place where it was declared.
    ///
    /// The frame draws the layer after the page, which is where a raster target needs it: above every box of the page, sticky ones included. A document needs it where it was declared instead, because there the order of the elements is the order Tab walks and a reader reads, so it puts the element in that place and fixes it against the viewport.
    pub fixed_in_place_of: Option<ElementId>,
}

/// A box that is a linked picture, as a document is told about it.
#[derive(Clone, PartialEq, Debug)]
pub struct Picture {
    pub source: crate::Linked,
    /// The picture's own size in pixels, which a document reserves before it has arrived.
    pub width: u32,
    pub height: u32,
    /// How it fills its box, as CSS `object-fit` names it.
    pub fit: &'static str,
    /// Wanted as soon as the page is: fetched eagerly and first, rather than when it comes near the view.
    pub priority: bool,
}

impl Element {
    pub fn new(
        id: ElementId,
        semantics: Semantics,
        layout: impl Into<Box<str>>,
        rect: Rect,
    ) -> Self {
        Self {
            id,
            semantics,
            layout: layout.into(),
            rect,
            scroll_to: None,
            primary_scroll: false,
            arrival_margin: Insets::default(),
            picture: None,
            fixed_in_place_of: None,
        }
    }

    /// Says the box is a layer fixed over the surface, declared where `place` stands. See [`Element::fixed_in_place_of`].
    pub fn fixed_in_place_of(mut self, place: ElementId) -> Self {
        self.fixed_in_place_of = Some(place);
        self
    }

    /// Says the box is `picture`; see [`Element::picture`].
    pub fn showing(mut self, picture: Picture) -> Self {
        self.picture = Some(std::sync::Arc::new(picture));
        self
    }

    /// Asks the backend to put this box's own scroll at `offset`. See [`Element::scroll_to`].
    pub fn asking_to_scroll(mut self, offset: Option<(f32, f32)>) -> Self {
        self.scroll_to = offset;
        self
    }

    /// Says how far short of its view's edges this box's scroll stops what it brings into view. See [`Element::arrival_margin`].
    pub fn arriving_within(mut self, margin: Insets) -> Self {
        self.arrival_margin = margin;
        self
    }

    /// Marks this box as the surface's primary scroll, or not. See [`Element::primary_scroll`].
    pub fn as_primary_scroll(mut self, primary: bool) -> Self {
        self.primary_scroll = primary;
        self
    }
}
