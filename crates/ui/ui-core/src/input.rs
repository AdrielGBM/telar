//! [`Input`]: the single-line text field — caret, selection, clipboard and the keys that drive them.

use std::rc::Rc;

use geometry_core::Rect;
use layout_core::{LayoutError, LayoutStyle};
use platform_core::{Event, Key, ModifiersState, NamedKey, PointerButton};
use reactive_core::{Effect, RwSignal, effect, signal};
use renderer_core::{RectStyle, ShapeStyle, TextStyle};
use ui_tree::{Component, EventResult, RenderNode};

use crate::caret::{Blink, align_origin};
use crate::focus::{self, FocusId};
use crate::impl_leaf_widget;
use crate::layout_leaf::LayoutLeaf;

/// Width of the caret, in logical px.
const CARET_WIDTH: f32 = 1.5;

/// A single-line editable text field bound to a `RwSignal<String>`. A base primitive: unstyled (no border or background — wrap it in a `box` for the look) and keyboard-driven. It requests focus on tap and, while focused, edits the bound signal from key events, drawing a caret at the insertion point. Selection (`Shift`+arrows/Home/End, `Ctrl+A`) with copy, cut and paste; IME composition is not yet supported. Drag-to-select waits on click-to-position, which this field does not have either.
pub struct Input {
    value: RwSignal<String>,
    // Reactive so a bare caret move re-renders even when the text is unchanged; always re-snapped to a char boundary in case the signal changed elsewhere.
    caret: RwSignal<usize>,
    // Either side of the caret: a selection extended leftwards has its anchor after it.
    anchor: RwSignal<Option<usize>>,
    style: Rc<dyn Fn() -> TextStyle>,
    id: FocusId,
    leaf: LayoutLeaf,
    on_submit: Option<Box<dyn Fn()>>,
    on_cancel: Option<Box<dyn Fn()>>,
    // Rendered in place of the text so the field stays live and tappable when empty — a separate placeholder widget swapped in would not take focus.
    placeholder: String,
    // Rendering only: the bound signal, the caret offsets and every edit still work on the real text.
    mask: Option<char>,
    blink: Blink,
    // Keeps the blink running while the field holds the keyboard, and stops it when it does not.
    _blinking: Effect,
}

impl Input {
    /// A single-line field, and whether the keyboard is in it.
    ///
    /// A target that draws its own caret needs neither; one that hands the box to a document needs both, so the document's own focus lands on the field a person clicked into rather than beside it.
    fn semantics(&self) -> renderer_core::Semantics {
        renderer_core::Semantics::of(renderer_core::Role::TextInput).in_state(
            focus::is_focused(self.id),
            None,
            false,
        )
    }

    pub fn new(
        value: RwSignal<String>,
        layout_style: LayoutStyle,
        style_fn: impl Fn() -> TextStyle + 'static,
    ) -> Result<Self, LayoutError> {
        Self::build(value, layout_style, |_| Rc::new(style_fn))
    }

    /// A field styled by what the tree above it declared, amended by whatever it says for itself — the counterpart of [`Text::declaring`](crate::Text::declaring), and what keeps a field's text the same size as the labels beside it.
    pub fn declaring(
        value: RwSignal<String>,
        layout_style: LayoutStyle,
        style_fn: impl Fn(TextStyle) -> TextStyle + 'static,
    ) -> Result<Self, LayoutError> {
        Self::build(value, layout_style, |node| {
            crate::inherit::inheriting(node, style_fn)
        })
    }

    fn build(
        value: RwSignal<String>,
        layout_style: LayoutStyle,
        style: impl FnOnce(layout_core::NodeId) -> Rc<dyn Fn() -> TextStyle>,
    ) -> Result<Self, LayoutError> {
        let leaf = LayoutLeaf::register(layout_style)?;
        let caret = value.with(|s| s.len());
        let id = focus::next_id();
        // As the kind that takes keys as text, so an app-level shortcut table stands aside while the caret is here.
        focus::register_at(id, focus::FocusKind::TextEntry, leaf.node);
        let blink = Blink::new();
        let watching = blink.clone();
        Ok(Self {
            value,
            caret: signal(caret),
            anchor: signal(None),
            style: style(leaf.node),
            id,
            leaf,
            on_submit: None,
            on_cancel: None,
            placeholder: String::new(),
            mask: None,
            blink,
            _blinking: effect(move || watching.follow(focus::is_focused(id))),
        })
    }

    /// Draws `bullet` in place of every character, for a password or a PIN.
    ///
    /// Rendering only — the bound signal keeps the real text, so a submit handler reads what was typed. Worth having as a property of the field rather than as a caller-side transformation: a caller that masked the *signal* would have to keep a second copy of the truth, and the caret would measure the wrong string.
    pub fn masked(mut self, bullet: char) -> Self {
        self.mask = Some(bullet);
        self
    }

    /// [`masked`](Self::masked) with the conventional bullet.
    pub fn secret(self) -> Self {
        self.masked('•')
    }

    /// What is drawn for `text`: the text itself, or one mask character per character of it.
    fn shown(&self, text: &str) -> String {
        match self.mask {
            Some(bullet) => text.chars().map(|_| bullet).collect(),
            None => text.to_string(),
        }
    }

    /// Runs when Enter is pressed while focused (e.g. submit a form / run a search).
    pub fn on_submit(mut self, f: impl Fn() + 'static) -> Self {
        self.on_submit = Some(Box::new(f));
        self
    }

    /// Runs when Escape is pressed while focused, just before the field hands the keyboard back.
    ///
    /// **The other half of [`on_submit`](Self::on_submit).** A field that can be committed but not abandoned is half a contract, and the missing half is the one a caller cannot write for itself: Escape is a key a focused field eats, so an application watching from outside sees the keyboard leave and has no way to tell «they gave up» from «they clicked somewhere else» — opposite answers wherever losing focus commits.
    pub fn on_cancel(mut self, f: impl Fn() + 'static) -> Self {
        self.on_cancel = Some(Box::new(f));
        self
    }

    /// A muted hint shown while the value is empty (the field stays tappable/focusable, unlike a swapped-in placeholder widget).
    pub fn placeholder(mut self, p: impl Into<String>) -> Self {
        self.placeholder = p.into();
        self
    }

    /// Gives this field keyboard focus as it is built, so the surface it is on is typed into rather than clicked into first.
    ///
    /// A field is otherwise focused only by a tap, which is the right default for a form but wrong for the surface that exists *because* it wants a keystroke — a search overlay opened on a keybind, a password prompt. Registration happens in [`new`](Self::new), so this is a request against an id that is already in the tab order.
    pub fn autofocus(self) -> Self {
        focus::request(self.id);
        self
    }

    /// The id this field holds in the tab order.
    ///
    /// For the caller that has to answer «who has the keyboard» about a field it did not build — a tab that turns into one, a cell edited in place — and cannot ask [`focus::current`] instead: that says who holds it now, which is the same answer for every field on the surface. `focus_id:$sig` in a `[view]` is this, mirrored into a signal and withdrawn when the field goes.
    pub fn focus_id(&self) -> focus::FocusId {
        self.id
    }

    /// The current caret byte offset, clamped to the text and snapped to a char boundary.
    fn caret_at(&self, text: &str) -> usize {
        let mut c = self.caret.get().min(text.len());
        while c > 0 && !text.is_char_boundary(c) {
            c -= 1;
        }
        c
    }

    /// The selected byte range, low end first, or `None` when nothing is selected. An anchor sitting on the caret is not a selection — it is where one would start from.
    fn selection(&self, text: &str) -> Option<(usize, usize)> {
        let caret = self.caret_at(text);
        let mut anchor = self.anchor.get()?.min(text.len());
        while anchor > 0 && !text.is_char_boundary(anchor) {
            anchor -= 1;
        }
        (anchor != caret).then(|| (anchor.min(caret), anchor.max(caret)))
    }

    /// The selected text, for a copy or a cut.
    fn selected_text(&self, text: &str) -> Option<String> {
        self.selection(text)
            .map(|(from, to)| text[from..to].to_string())
    }

    /// Removes the selection from `text` and reports where the caret lands, or `None` when there was none. Every edit runs through this first: typing over a selection replaces it, which is the behaviour that makes a selection worth having.
    fn take_selection(&self, text: &mut String) -> Option<usize> {
        let (from, to) = self.selection(text)?;
        text.replace_range(from..to, "");
        Some(from)
    }

    /// Applies a key while focused, editing the bound signal and/or moving the caret. Returns whether the key was consumed.
    fn edit(&mut self, key: &Key, mods: &ModifiersState) -> EventResult {
        // Before the key is even read: a caret that blinked out under the hand is missing at the one moment somebody is looking for it.
        self.blink.wake();
        let mut text = self.value.get();
        let mut caret = self.caret_at(&text);
        let chord = mods.is_ctrl || mods.is_meta;
        // Set after the match so each arm can still read the selection it is replacing.
        let mut anchor = if mods.is_shift {
            Some(self.anchor.get().unwrap_or(caret))
        } else {
            None
        };
        match key {
            Key::Char('a') | Key::Char('A') if chord => {
                anchor = Some(0);
                caret = text.len();
            }
            Key::Char('c') | Key::Char('C') if chord => {
                let Some(selected) = self.selected_text(&text) else {
                    return EventResult::Ignored;
                };
                services_core::set_clipboard_text(&selected);
                return EventResult::Handled;
            }
            Key::Char('x') | Key::Char('X') if chord => {
                let Some(selected) = self.selected_text(&text) else {
                    return EventResult::Ignored;
                };
                services_core::set_clipboard_text(&selected);
                caret = self.take_selection(&mut text).unwrap_or(caret);
            }
            Key::Char('v') | Key::Char('V') if chord => {
                let Some(pasted) = services_core::clipboard_text() else {
                    return EventResult::Ignored;
                };
                // A multi-line paste would otherwise put a `\n` in a value nothing can render, bound to a signal something else reads.
                let pasted = pasted.lines().next().unwrap_or_default().to_string();
                let had_selection = self.take_selection(&mut text);
                if pasted.is_empty() && had_selection.is_none() {
                    return EventResult::Ignored;
                }
                caret = had_selection.unwrap_or(caret);
                text.insert_str(caret, &pasted);
                caret += pasted.len();
            }
            // Any other chord is a shortcut, not text.
            Key::Char(_) if chord => return EventResult::Ignored,
            Key::Char(c) if !c.is_control() => {
                caret = self.take_selection(&mut text).unwrap_or(caret);
                text.insert(caret, *c);
                caret += c.len_utf8();
            }
            Key::Named(NamedKey::Space) => {
                caret = self.take_selection(&mut text).unwrap_or(caret);
                text.insert(caret, ' ');
                caret += 1;
            }
            Key::Named(NamedKey::Backspace) => {
                if let Some(at) = self.take_selection(&mut text) {
                    caret = at;
                } else {
                    if caret == 0 {
                        return EventResult::Ignored;
                    }
                    let prev = prev_boundary(&text, caret);
                    text.replace_range(prev..caret, "");
                    caret = prev;
                }
            }
            Key::Named(NamedKey::Delete) => {
                if let Some(at) = self.take_selection(&mut text) {
                    caret = at;
                } else {
                    if caret >= text.len() {
                        return EventResult::Ignored;
                    }
                    let next = next_boundary(&text, caret);
                    text.replace_range(caret..next, "");
                }
            }
            // Pressing Left with three characters selected puts the caret before them, not inside them.
            Key::Named(NamedKey::ArrowLeft) => {
                caret = match self.selection(&text) {
                    Some((from, _)) if !mods.is_shift => from,
                    _ => prev_boundary(&text, caret),
                }
            }
            Key::Named(NamedKey::ArrowRight) => {
                caret = match self.selection(&text) {
                    Some((_, to)) if !mods.is_shift => to,
                    _ => next_boundary(&text, caret),
                }
            }
            Key::Named(NamedKey::Home) => caret = 0,
            Key::Named(NamedKey::End) => caret = text.len(),
            Key::Named(NamedKey::Enter) => {
                if let Some(cb) = &self.on_submit {
                    cb();
                }
                return EventResult::Handled;
            }
            Key::Named(NamedKey::Escape) => {
                // Before the keyboard goes back, so a caller watching focus reads what follows as the consequence.
                if let Some(cb) = &self.on_cancel {
                    cb();
                }
                focus::release(self.id);
                return EventResult::Handled;
            }
            Key::Named(NamedKey::Tab) => {
                if mods.is_shift {
                    focus::focus_prev();
                } else {
                    focus::focus_next();
                }
                return EventResult::Handled;
            }
            _ => return EventResult::Ignored,
        }
        // So a bare caret move does not rebuild the string.
        let edited = self.value.with(|s| s != &text);
        if edited {
            self.value.set(text);
        }
        self.caret.set(caret);
        // An anchor that caught up with the caret is no selection, and keeping it would make the next unshifted arrow collapse to a range of nothing. An edit drops it outright: kept, the anchor sat where the caret was before the letter went in, so the next keystroke replaced what had just been typed.
        self.anchor.set(anchor.filter(|a| *a != caret && !edited));
        EventResult::Handled
    }
}

impl Component for Input {
    fn view(&self) -> RenderNode {
        let r = self.leaf.rect.get();
        let text = self.value.get();
        let style = (self.style)();
        let paint = style.color;
        let full = Rect {
            x: 0.0,
            y: 0.0,
            width: r.width,
            height: r.height,
        };
        // The field itself stays live: the caret and hit-test still work, so it is typable from empty.
        let text_node = if text.is_empty() && !self.placeholder.is_empty() {
            let mut ph_style = style.clone();
            ph_style.color = style.color.faded(0.5);
            RenderNode::text(self.placeholder.clone(), full, ph_style)
        } else {
            RenderNode::text(self.shown(&text), full, style.clone())
        };

        // Reading `is_focused` subscribes this view to focus moves.
        if focus::is_focused(self.id) {
            let caret = self.caret_at(&text);
            // Everything drawn beside the letters is placed from where the shaper puts the first glyph, or a field inheriting a centred alignment draws its text in the middle and its caret at the left.
            let line = self.shown(&text);
            let (line_w, _) = crate::text_metrics::measure_text(&line, None, 1.0e6, &style);
            let origin = align_origin(style.text_align, full.width, line_w);
            // Behind the text, in the ink at low alpha: a field is unstyled by design and has no palette, and the ink is the one colour it is guaranteed to contrast with.
            let highlight = self.selection(&text).map(|(from, to)| {
                let measure = |upto: usize| {
                    crate::text_metrics::measure_text(
                        &self.shown(&text[..upto]),
                        None,
                        1.0e6,
                        &style,
                    )
                    .0
                };
                let (start, end) = (measure(from), measure(to));
                let fill = style.color.faded(0.25);
                RenderNode::rect(
                    Rect {
                        x: origin + start,
                        y: 0.0,
                        width: (end - start).max(1.0),
                        height: crate::text_metrics::line_box(&style),
                    },
                    RectStyle::default().with_fill(fill),
                )
            });
            // A mask character is not the width of what it hides, so measuring the real prefix would put the caret somewhere the text is not.
            let prefix = self.shown(&text[..caret]);
            let (prefix_w, _) = crate::text_metrics::measure_text(&prefix, None, 1.0e6, &style);
            let line_h = crate::text_metrics::line_box(&style);
            let caret_rect = Rect {
                x: origin + prefix_w,
                y: 0.0,
                width: CARET_WIDTH,
                height: line_h,
            };
            // Read here and nowhere else, so the caret is the only thing on the surface redrawing on the blink's account.
            let lit = paint.faded(self.blink.opacity());
            let caret_node = RenderNode::rect(caret_rect, RectStyle::default().with_fill(lit));
            let layers = match highlight {
                Some(highlight) => vec![highlight, text_node, caret_node],
                None => vec![text_node, caret_node],
            };
            self.leaf
                .at_layout_position_as(|| self.semantics(), RenderNode::group(layers))
        } else {
            self.leaf
                .at_layout_position_as(|| self.semantics(), text_node)
        }
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        let rect = self.leaf.rect.get();
        match event {
            Event::PointerPressed {
                x,
                y,
                button: PointerButton::Primary,
                ..
            } => {
                if rect.contains(*x as f32, *y as f32) {
                    focus::request_from_pointer(self.id);
                    // Click-to-position needs per-glyph measurement, and drag-to-select waits on it: there is no x-to-offset mapping to drag along yet.
                    self.caret.set(self.value.with(|s| s.len()));
                    self.anchor.set(None);
                    EventResult::Handled
                } else {
                    EventResult::Ignored
                }
            }
            Event::KeyPressed { key, modifiers } if focus::is_focused(self.id) => {
                self.edit(key, modifiers)
            }
            _ => EventResult::Ignored,
        }
    }

    fn debug_name(&self) -> &'static str {
        "Input"
    }
}

impl Drop for Input {
    fn drop(&mut self) {
        // Leaves the tab order, and drops focus if held, when a reactive list destroys the field.
        focus::unregister(self.id);
    }
}

impl_leaf_widget!(Input);

/// The char boundary strictly before byte offset `i` (or 0).
fn prev_boundary(s: &str, i: usize) -> usize {
    let mut j = i.min(s.len());
    if j == 0 {
        return 0;
    }
    j -= 1;
    while j > 0 && !s.is_char_boundary(j) {
        j -= 1;
    }
    j
}

/// The char boundary strictly after byte offset `i` (or `s.len()`).
fn next_boundary(s: &str, i: usize) -> usize {
    let mut j = (i + 1).min(s.len());
    while j < s.len() && !s.is_char_boundary(j) {
        j += 1;
    }
    j
}

#[cfg(test)]
#[path = "input_test.rs"]
mod tests;
