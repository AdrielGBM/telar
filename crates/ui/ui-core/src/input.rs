//! [`Input`]: the single-line text field.

use std::rc::Rc;

use layout_core::{LayoutError, LayoutStyle};
use platform_core::{Event, Key, ModifiersState};
use reactive_core::{RwSignal, effect};
use renderer_core::TextStyle;
use ui_tree::{Component, EventResult, RenderNode};

use crate::focus;
use crate::layout_item::LeafWidget;
use crate::layout_leaf::LayoutLeaf;
use crate::text_editor::{Caret, Editor, Lines, Underline};

/// A single-line editable text field bound to a `RwSignal<String>`. A base primitive: unstyled (no border or background — wrap it in a `box` for the look). It requests focus on a press, and edits the bound signal from key events while focused, drawing a caret at the insertion point.
///
/// The line never wraps: one longer than the box is cut at its edges and scrolls sideways under it, so the caret and what an input method is composing are always in view. A press puts the caret where it lands, a drag selects from there, a double-click takes a word and a triple-click the whole line, and Shift with a press extends the selection; so do `Shift`+arrows/Home/End and `Mod+A`, with copy, cut and paste on the primary modifier, Command on a Mac and Ctrl elsewhere. An input method composes in place: what it has not decided on yet is drawn at the caret, underlined, and what it commits is typed there (see `docs/keyboard.md`). Enter runs [`on_submit`](Self::on_submit) and Tab moves on; [`TextArea`](crate::TextArea) is the same editor with several lines.
pub struct Input {
    pub(crate) editor: Editor,
}

impl Input {
    pub fn new(
        value: RwSignal<String>,
        layout_style: LayoutStyle,
        style_fn: impl Fn() -> TextStyle + 'static,
    ) -> Result<Self, LayoutError> {
        Self::build(value, layout_style, Rc::new(style_fn))
    }

    /// A field styled by what the tree above it declared, amended by whatever it says for itself — the counterpart of [`Text::declaring`](crate::Text::declaring), and what keeps a field's text the same size as the labels beside it.
    pub fn declaring(
        value: RwSignal<String>,
        layout_style: LayoutStyle,
        style_fn: impl Fn(TextStyle) -> TextStyle + 'static,
    ) -> Result<Self, LayoutError> {
        let editor = Editor::build(value, layout_style, Lines::Single, |node| {
            Rc::new(move || style_fn(crate::inherit::inherited_text_style_at(node.get())))
        })?;
        Ok(Self { editor })
    }

    fn build(
        value: RwSignal<String>,
        layout_style: LayoutStyle,
        style: Rc<dyn Fn() -> TextStyle>,
    ) -> Result<Self, LayoutError> {
        let editor = Editor::build(value, layout_style, Lines::Single, |_| style)?;
        Ok(Self { editor })
    }

    /// Draws `bullet` in place of every character, for a password or a PIN.
    ///
    /// Rendering only — the bound signal keeps the real text, so a submit handler reads what was typed. Worth having as a property of the field rather than as a caller-side transformation: a caller that masked the *signal* would have to keep a second copy of the truth, and the caret would measure the wrong string.
    pub fn masked(self, bullet: char) -> Self {
        self.editor.set_mask(bullet);
        self
    }

    /// [`masked`](Self::masked) with the conventional bullet.
    pub fn secret(self) -> Self {
        self.masked('•')
    }

    /// Runs when Enter is pressed while focused (e.g. submit a form / run a search).
    pub fn on_submit(mut self, f: impl Fn() + 'static) -> Self {
        self.editor.on_submit = Some(Box::new(f));
        self
    }

    /// Runs when Escape is pressed while focused, just before the field hands the keyboard back.
    ///
    /// **The other half of [`on_submit`](Self::on_submit).** A field that can be committed but not abandoned is half a contract, and the missing half is the one a caller cannot write for itself: Escape is a key a focused field eats, so an application watching from outside sees the keyboard leave and has no way to tell «they gave up» from «they clicked somewhere else» — opposite answers wherever losing focus commits.
    pub fn on_cancel(mut self, f: impl Fn() + 'static) -> Self {
        self.editor.on_cancel = Some(Box::new(f));
        self
    }

    /// Hears every key pressed while the field holds the keyboard, before the field acts on it; answering `true` takes the key from the field.
    ///
    /// What a field with suggestions under it needs: while they show, the arrows walk them and Enter, Tab and Escape choose or close them — keys the field uses itself, which a handler around it never hears because a focused field keeps them.
    pub fn on_key(mut self, f: impl Fn(&Key, ModifiersState) -> bool + 'static) -> Self {
        self.editor.on_key = Some(Box::new(f));
        self
    }

    /// Draws a line under each run `spans` answers, re-read every frame: where a checker found something wrong.
    pub fn underline(mut self, spans: impl Fn() -> Vec<Underline> + 'static) -> Self {
        self.editor.underlines = Some(Box::new(spans));
        self
    }

    /// The insertion point, to read and to move from outside.
    pub fn caret(&self) -> Caret {
        self.editor.caret_handle()
    }

    /// A muted hint shown while the value is empty (the field stays tappable/focusable, unlike a swapped-in placeholder widget).
    pub fn placeholder(self, p: impl Into<String>) -> Self {
        self.editor.set_placeholder(p.into());
        self
    }

    /// Gives this field keyboard focus as it is built, so the surface it is on is typed into rather than clicked into first.
    ///
    /// A field is otherwise focused only by a tap, which is the right default for a form but wrong for the surface that exists *because* it wants a keystroke — a search overlay opened on a keybind, a password prompt. Registration happens in [`new`](Self::new), so this is a request against an id that is already in the tab order.
    pub fn autofocus(self) -> Self {
        focus::request(self.editor.id);
        self
    }

    /// Selects the whole text each time the field takes focus, so typing replaces it — what a value opened for editing in place wants.
    pub fn select_on_focus(self) -> Self {
        let (id, value, caret, anchor) = (
            self.editor.id,
            self.editor.value,
            self.editor.caret,
            self.editor.anchor,
        );
        let had = std::cell::Cell::new(focus::is_focused(id));
        effect(move || {
            let now = focus::is_focused(id);
            if now && !had.get() {
                caret.set(value.peek().len());
                anchor.set(Some(0));
            }
            had.set(now);
        });
        self
    }

    /// The id this field holds in the tab order.
    ///
    /// For the caller that has to answer «who has the keyboard» about a field it did not build — a tab that turns into one, a cell edited in place — and cannot ask [`focus::current`] instead: that says who holds it now, which is the same answer for every field on the surface. `focus_id:$sig` in a `[view]` is this, mirrored into a signal and withdrawn when the field goes.
    pub fn focus_id(&self) -> focus::FocusId {
        self.editor.id
    }
}

impl Component for Input {
    fn view(&self) -> RenderNode {
        self.editor.view()
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        self.editor.on_event(event)
    }

    fn debug_name(&self) -> &'static str {
        "Input"
    }
}

impl LeafWidget for Input {
    fn layout_leaf(&self) -> &LayoutLeaf {
        &self.editor.leaf
    }
}

#[cfg(test)]
#[path = "input_test.rs"]
mod tests;
