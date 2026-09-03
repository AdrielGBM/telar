//! What a drawn box *is*, for a backend that reconciles elements rather than rasterising pixels.
//!
//! A raster backend needs none of this: it is handed rects that are already where they belong. A backend
//! whose output is a document needs to know which box is which between frames, so it can move an element
//! rather than rebuild it — and needs to know what a box *means*, so a button is a `<button>` and not a
//! `<div>` that happens to be clickable.

use geometry_core::Rect;
use std::sync::Arc;

/// Identifies one box across frames.
///
/// The layout node's own id: it is created with the widget, lives as long as it, and is already the thing
/// every other layer uses to name that box. Minting a second identity would only be one more thing to keep
/// in step.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct ElementId(pub u64);

/// What a box is, beyond a rectangle.
///
/// Deliberately small. Each variant has to earn itself by changing what a document backend emits — a role
/// that lands on the same element with the same attributes is a role that does not exist.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
pub enum Role {
    /// A box that groups. The overwhelming majority.
    #[default]
    Group,
    /// Something pressable, whatever it is drawn as.
    Button,
    /// A link, and where it goes.
    Link(Arc<str>),
    /// A heading, and how deep. `1` is the page's own title.
    Heading(u8),
    /// A single-line editable field.
    TextInput,
    /// A picture.
    Image,
    /// A region that scrolls its content.
    ScrollArea,
}

/// What a box is and how it should be described, carried alongside its geometry.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct Semantics {
    pub role: Role,
    /// The name assistive technology reads, when the box's own content is not it — an icon-only button.
    pub label: Option<Arc<str>>,
    /// Whether the box refuses pointer events, so what is drawn under it takes them instead.
    pub click_through: bool,
}

impl Semantics {
    pub fn group() -> Self {
        Self::default()
    }

    pub fn with_role(mut self, role: Role) -> Self {
        self.role = role;
        self
    }

    pub fn with_label(mut self, label: impl Into<Arc<str>>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn click_through(mut self) -> Self {
        self.click_through = true;
        self
    }
}

/// One box in a frame: what it is, what it was asked for, and what to call it.
///
/// Carries its own layout rather than leaving a document backend to look it up. Two things follow. The
/// backend becomes a pure function of the command stream — it needs no access to the layout engine, and can
/// therefore be tested against a stream built on a machine with no browser. And the string is built where
/// the style is already known, once per re-render of the widget that owns it, rather than once per frame.
#[derive(Clone, PartialEq, Debug)]
pub struct Element {
    pub id: ElementId,
    pub semantics: Semantics,
    /// The box's layout, as CSS declarations — `display:flex;gap:8px;` and so on. Empty where the target
    /// does not want them, which is every target that positions the box itself.
    pub layout: Box<str>,
    /// Where layout put the box, in the surface's own coordinates.
    ///
    /// Carried alongside the declarations, not instead of them, because the two answer different questions.
    /// A box inside another box is placed by its parent, and the declarations are what the parent needs. A
    /// box that *is* a layout root — an application that computes several and places them itself — has no
    /// parent to place it, and a document told only what it asked for would stack them.
    pub rect: Rect,
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
        }
    }
}
