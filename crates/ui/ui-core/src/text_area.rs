//! [`TextArea`]: the multi-line editor, with a height measured from the rows it is drawn in.

use std::rc::Rc;

use layout_core::{LayoutError, LayoutStyle};
use platform_core::{Event, Key, ModifiersState};
use reactive_core::RwSignal;
use renderer_core::{TextStyle, TextWrap};
use ui_tree::{Component, EventResult, RenderNode};

use crate::focus::{self, FocusId};
use crate::layout_item::LeafWidget;
use crate::layout_leaf::LayoutLeaf;
use crate::text_editor::{Caret, Editor, Lines, Underline};

/// A multi-line editable text area bound to a `RwSignal<String>` — the multi-line sibling of [`Input`](crate::Input), and the same editor underneath: the caret, the selection, the presses, drags, double- and triple-clicks that make one, the clipboard and an input method's composition behave alike in both. A base primitive: unstyled (wrap it in a `box` for a border/background). Enter breaks the line, Backspace/Delete join lines, the arrows move in all four directions, Home/End go to the ends of the row and Tab inserts spaces; a triple-click takes the line it lands in.
///
/// By default only `\n` breaks a line, and one longer than the box is cut at its edge and scrolls sideways under it, which is what a code editor and a line-number gutter beside it expect. [`text_wrap`](Self::text_wrap) with [`TextWrap::Wrap`] breaks lines at the area's width instead, between words and inside a word too long for a row, as a paragraph and an HTML `<textarea>` do; the caret then moves by the rows on screen. Its measured height grows with the rows it is drawn in, so wrapping it in a [`LayoutScrollArea`](crate::LayoutScrollArea) gives a scrolling editor whose view follows the caret.
pub struct TextArea {
    pub(crate) editor: Editor,
}

impl TextArea {
    /// An area in exactly `style_fn`'s style: the opt-out from the tree above, the surface's family included. [`declaring`](Self::declaring) is the one that follows it.
    pub fn new(
        value: RwSignal<String>,
        layout_style: LayoutStyle,
        style_fn: impl Fn() -> TextStyle + 'static,
    ) -> Result<Self, LayoutError> {
        let style: Rc<dyn Fn() -> TextStyle> = Rc::new(style_fn);
        let editor = Editor::build(value, layout_style, Lines::Multi, |_| style)?;
        Ok(Self { editor })
    }

    /// An area styled by what the tree above it declared, amended by whatever it says for itself — the counterpart of [`Text::declaring`](crate::Text::declaring) and [`Input::declaring`](crate::Input::declaring), and what keeps an editor in the surface's family ([`set_font_family`](crate::set_font_family)) and at the size of the text around it.
    pub fn declaring(
        value: RwSignal<String>,
        layout_style: LayoutStyle,
        style_fn: impl Fn(TextStyle) -> TextStyle + 'static,
    ) -> Result<Self, LayoutError> {
        let editor = Editor::build(value, layout_style, Lines::Multi, |node| {
            Rc::new(move || style_fn(crate::inherit::inherited_text_style_at(node.get())))
        })?;
        Ok(Self { editor })
    }

    /// A muted hint shown while the value is empty, wrapped as the text would be.
    pub fn placeholder(self, p: impl Into<String>) -> Self {
        self.editor.set_placeholder(p.into());
        self
    }

    /// Whether lines break at the area's width ([`TextWrap::Wrap`]) or only where a `\n` breaks them ([`TextWrap::NoWrap`], the default), whatever the style around the area says.
    pub fn text_wrap(self, wrap: TextWrap) -> Self {
        self.editor.set_wrap(wrap);
        self
    }

    /// Runs when Escape is pressed while focused, just before the area hands the keyboard back. See [`Input::on_cancel`](crate::Input::on_cancel).
    pub fn on_cancel(mut self, f: impl Fn() + 'static) -> Self {
        self.editor.on_cancel = Some(Box::new(f));
        self
    }

    /// Hears every key pressed while the area holds the keyboard, before the area acts on it; answering `true` takes the key from it. See [`Input::on_key`](crate::Input::on_key).
    pub fn on_key(mut self, f: impl Fn(&Key, ModifiersState) -> bool + 'static) -> Self {
        self.editor.on_key = Some(Box::new(f));
        self
    }

    /// Draws a line under each run `spans` answers, re-read every frame, on every row the run is drawn across.
    pub fn underline(mut self, spans: impl Fn() -> Vec<Underline> + 'static) -> Self {
        self.editor.underlines = Some(Box::new(spans));
        self
    }

    /// The insertion point, to read and to move from outside.
    pub fn caret(&self) -> Caret {
        self.editor.caret_handle()
    }

    /// Gives keyboard focus to this area (as a tap would), leaving the caret where it was. For programmatic focus — e.g. a container autofocusing the editor when its tab or window becomes active.
    pub fn request_focus(&self) {
        focus::request(self.editor.id);
    }

    /// Whether this area currently holds keyboard focus.
    pub fn focused(&self) -> bool {
        focus::is_focused(self.editor.id)
    }

    /// A `Copy` [`focus::FocusHandle`] to this area, so a caller that has moved it into a container can still focus it later (e.g. autofocus on tab activation) without keeping a reference to the area itself.
    pub fn focus_handle(&self) -> focus::FocusHandle {
        focus::handle(self.editor.id)
    }

    /// The id this area holds in the tab order, for a box that frames it and has to know when it holds the keyboard. See [`Input::focus_id`](crate::Input::focus_id).
    pub fn focus_id(&self) -> FocusId {
        self.editor.id
    }
}

impl Component for TextArea {
    fn view(&self) -> RenderNode {
        self.editor.view()
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        self.editor.on_event(event)
    }

    fn debug_name(&self) -> &'static str {
        "TextArea"
    }
}

impl LeafWidget for TextArea {
    fn layout_leaf(&self) -> &LayoutLeaf {
        &self.editor.leaf
    }
}

#[cfg(test)]
#[path = "text_area_test.rs"]
mod tests;
