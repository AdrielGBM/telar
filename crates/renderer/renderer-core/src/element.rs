//! What a drawn box *is*, for a backend that reconciles elements rather than rasterising pixels.
//!
//! A raster backend needs none of this: it is handed rects that are already where they belong. A backend
//! whose output is a document needs to know which box is which between frames, so it can move an element
//! rather than rebuild it — and needs to know what a box *means*, so a button is a `<button>` and not a
//! `<div>` that happens to be clickable.

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
