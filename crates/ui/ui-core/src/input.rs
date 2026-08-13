use std::rc::Rc;

use geometry_core::Rect;
use layout_core::{LayoutError, LayoutStyle};
use platform_core::{Event, Key, ModifiersState, NamedKey, PointerButton};
use reactive_core::{RwSignal, signal};
use renderer_core::{Color, Paint, RectStyle, ShapeStyle, TextStyle};
use ui_tree::{Component, EventResult, RenderNode};

use crate::focus::{self, FocusId};
use crate::impl_leaf_widget;
use crate::layout_leaf::LayoutLeaf;

/// Width of the caret, in logical px.
const CARET_WIDTH: f32 = 1.5;

/// A single-line editable text field bound to a `RwSignal<String>`. A base primitive: unstyled (no
/// border or background — wrap it in a `box` for the look) and keyboard-driven. It requests focus on
/// tap and, while focused, edits the bound signal from key events, drawing a caret at the insertion
/// point. Selection, clipboard, and IME composition are not yet supported (a single-caret MVP).
pub struct Input {
    value: RwSignal<String>,
    // Caret byte offset into `value`. Reactive so a bare caret move (arrows/home/end) re-renders even
    // when the text is unchanged; always re-snapped to a char boundary in case the signal changed elsewhere.
    caret: RwSignal<usize>,
    style: Rc<dyn Fn() -> TextStyle>,
    id: FocusId,
    leaf: LayoutLeaf,
    on_submit: Option<Box<dyn Fn()>>,
    // Hint shown (muted) while the value is empty. Rendered in place of the text so the field stays a live,
    // tappable `Input` even when empty — a separate placeholder widget swapped in would not take focus.
    placeholder: String,
    // Character drawn in place of every character of the value. Affects rendering only: the bound signal, the
    // caret offsets and every edit still work on the real text.
    mask: Option<char>,
}

impl Input {
    pub fn new(
        value: RwSignal<String>,
        layout_style: LayoutStyle,
        style_fn: impl Fn() -> TextStyle + 'static,
    ) -> Result<Self, LayoutError> {
        let leaf = LayoutLeaf::register(layout_style)?;
        let caret = value.with(|s| s.len());
        let id = focus::next_id();
        // Join the tab order so Tab/Shift-Tab can reach this field, as the kind that takes keys as text — so
        // an app-level shortcut table can stand aside while the caret is here.
        focus::register_at(id, focus::FocusKind::TextEntry, leaf.node);
        Ok(Self {
            value,
            caret: signal(caret),
            style: Rc::new(style_fn),
            id,
            leaf,
            on_submit: None,
            placeholder: String::new(),
            mask: None,
        })
    }

    /// Draws `bullet` in place of every character, for a password or a PIN.
    ///
    /// Rendering only — the bound signal keeps the real text, so a submit handler reads what was typed. Worth
    /// having as a property of the field rather than as a caller-side transformation: a caller that masked the
    /// *signal* would have to keep a second copy of the truth, and the caret would measure the wrong string.
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

    /// A muted hint shown while the value is empty (the field stays tappable/focusable, unlike a swapped-in
    /// placeholder widget).
    pub fn placeholder(mut self, p: impl Into<String>) -> Self {
        self.placeholder = p.into();
        self
    }

    /// Gives this field keyboard focus as it is built, so the surface it is on is typed into rather than
    /// clicked into first.
    ///
    /// A field is otherwise focused only by a tap, which is the right default for a form but wrong for the
    /// surface that exists *because* it wants a keystroke — a search overlay opened on a keybind, a password
    /// prompt. Registration happens in [`new`](Self::new), so this is a request against an id that is already
    /// in the tab order.
    pub fn autofocus(self) -> Self {
        focus::request(self.id);
        self
    }

    /// Gives keyboard focus to this field (as a tap would). For programmatic focus after construction — e.g. a
    /// container focusing the field when its tab becomes active. Mirrors `TextArea::request_focus`.
    pub fn request_focus(&self) {
        focus::request(self.id);
    }

    /// Whether this field currently holds keyboard focus.
    pub fn focused(&self) -> bool {
        focus::is_focused(self.id)
    }

    /// A `Copy` [`focus::FocusHandle`] to this field, so a caller that has moved it into a container can still
    /// focus it later without keeping a reference to the field itself.
    pub fn focus_handle(&self) -> focus::FocusHandle {
        focus::handle(self.id)
    }

    /// The current caret byte offset, clamped to the text and snapped to a char boundary.
    fn caret_at(&self, text: &str) -> usize {
        let mut c = self.caret.get().min(text.len());
        while c > 0 && !text.is_char_boundary(c) {
            c -= 1;
        }
        c
    }

    /// Applies a key while focused, editing the bound signal and/or moving the caret. Returns whether the
    /// key was consumed.
    fn edit(&mut self, key: &Key, mods: &ModifiersState) -> EventResult {
        let mut text = self.value.get();
        let mut caret = self.caret_at(&text);
        match key {
            // A chord (Ctrl/Meta) is a shortcut, not text — leave it for global handlers (copy/paste TBD).
            Key::Char(_) if mods.is_ctrl || mods.is_meta => return EventResult::Ignored,
            Key::Char(c) if !c.is_control() => {
                text.insert(caret, *c);
                caret += c.len_utf8();
            }
            Key::Named(NamedKey::Space) => {
                text.insert(caret, ' ');
                caret += 1;
            }
            Key::Named(NamedKey::Backspace) => {
                if caret == 0 {
                    return EventResult::Ignored;
                }
                let prev = prev_boundary(&text, caret);
                text.replace_range(prev..caret, "");
                caret = prev;
            }
            Key::Named(NamedKey::Delete) => {
                if caret >= text.len() {
                    return EventResult::Ignored;
                }
                let next = next_boundary(&text, caret);
                text.replace_range(caret..next, "");
            }
            Key::Named(NamedKey::ArrowLeft) => caret = prev_boundary(&text, caret),
            Key::Named(NamedKey::ArrowRight) => caret = next_boundary(&text, caret),
            Key::Named(NamedKey::Home) => caret = 0,
            Key::Named(NamedKey::End) => caret = text.len(),
            Key::Named(NamedKey::Enter) => {
                if let Some(cb) = &self.on_submit {
                    cb();
                }
                return EventResult::Handled;
            }
            Key::Named(NamedKey::Escape) => {
                focus::release(self.id);
                return EventResult::Handled;
            }
            // Tab moves focus to the next/previous field instead of inserting a tab character.
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
        // Only push a new string when the text actually changed, so a bare caret move doesn't rebuild it.
        if self.value.with(|s| s != &text) {
            self.value.set(text);
        }
        self.caret.set(caret);
        EventResult::Handled
    }
}

impl Component for Input {
    fn view(&self) -> RenderNode {
        let r = self.leaf.rect.get();
        let text = self.value.get();
        let style = (self.style)();
        let full = Rect {
            x: 0.0,
            y: 0.0,
            width: r.width,
            height: r.height,
        };
        // Empty value → draw the muted placeholder in the text's place (the field itself stays live: the
        // caret and hit-test still work, so it's tappable/typable from empty).
        let text_node = if text.is_empty() && !self.placeholder.is_empty() {
            let muted = match style.paint {
                Paint::Solid(c) => Paint::Solid(c.with_alpha(c.a * 0.5)),
                _ => Paint::Solid(Color::rgba(0.5, 0.5, 0.55, 0.5)),
            };
            let mut ph_style = style;
            ph_style.paint = muted;
            RenderNode::text(self.placeholder.clone(), full, ph_style)
        } else {
            RenderNode::text(self.shown(&text), full, style)
        };

        // The caret is drawn only while focused; reading `is_focused` subscribes this view to focus moves.
        if focus::is_focused(self.id) {
            let caret = self.caret_at(&text);
            // Measured against what is *drawn*: a mask character is not the width of the character it hides,
            // so measuring the real prefix would put the caret somewhere the text is not.
            let prefix = self.shown(&text[..caret]);
            let (prefix_w, _) = renderer_text::measure_text(&prefix, 1.0e6, &style);
            let line_h = style.font_size * renderer_text::LINE_HEIGHT_FACTOR;
            let caret_rect = Rect {
                x: prefix_w,
                y: 0.0,
                width: CARET_WIDTH,
                height: line_h,
            };
            let caret_node =
                RenderNode::rect(caret_rect, RectStyle::default().with_fill(style.paint));
            self.leaf
                .at_layout_position(RenderNode::group([text_node, caret_node]))
        } else {
            self.leaf.at_layout_position(text_node)
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
                    // MVP: land the caret at the end. Click-to-position (measuring per glyph) is a follow-up.
                    self.caret.set(self.value.with(|s| s.len()));
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
        // Leave the tab order (and drop focus if held) when the field is destroyed, e.g. by a reactive list.
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
mod tests {
    use crate::context::reset_layout_runtime;
    use layout_core::AvailableSpace;
    use platform_core::PointerSource;
    use renderer_core::Color;

    use super::*;
    use crate::context::{compute_layout, new_container};
    use crate::layout_item::LayoutItem;

    fn key(k: Key) -> Event {
        Event::KeyPressed {
            key: k,
            modifiers: ModifiersState::default(),
        }
    }

    // Builds a focused, laid-out input bound to `initial` and returns it plus its value signal.
    fn focused_input(initial: &str) -> (Input, RwSignal<String>) {
        reset_layout_runtime();
        let value = signal(initial.to_string());
        let input = Input::new(
            value.clone(),
            LayoutStyle::new().width(200.0).height(20.0),
            || TextStyle::new(14.0, Color::BLACK),
        )
        .unwrap();
        let root = new_container(
            LayoutStyle::new().flex_column().width(200.0).height(100.0),
            &[input.layout_node()],
        )
        .unwrap();
        compute_layout(
            root,
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();
        focus::request(input.id);
        (input, value)
    }

    /// The guard a global shortcut handler consults ([`focus::text_entry_takes_key`]) is a second list of
    /// what this editor eats, kept apart from `edit` because it has to answer without running the edit. A key
    /// added here and not there re-opens the bug it exists for: typing that also fires the app's shortcuts.
    #[test]
    fn the_shortcut_guard_covers_every_key_this_field_edits() {
        let plain = ModifiersState::default();
        let named = [
            NamedKey::Space,
            NamedKey::Backspace,
            NamedKey::Delete,
            NamedKey::ArrowLeft,
            NamedKey::ArrowRight,
            NamedKey::ArrowUp,
            NamedKey::ArrowDown,
            NamedKey::Home,
            NamedKey::End,
            NamedKey::Enter,
            NamedKey::Escape,
            NamedKey::Tab,
            NamedKey::PageUp,
            NamedKey::PageDown,
            NamedKey::F5,
            NamedKey::Insert,
        ];
        let keys: Vec<Key> = std::iter::once(Key::Char('3'))
            .chain(std::iter::once(Key::Char('s')))
            .chain(named.into_iter().map(Key::Named))
            .collect();
        for k in keys {
            // A fresh field per key: Escape and Tab move focus, and an edit changes what the next key does.
            let (mut input, _value) = focused_input("hello");
            input.caret.set(2);
            // Asked first, as dispatch does: a global handler decides before the field acts, and Escape is
            // the key that proves it — the field answers it by giving up the focus the guard reads.
            let guarded = focus::text_entry_takes_key(&k, plain);
            let edited = input.edit(&k, &plain) == EventResult::Handled;
            assert!(
                !edited || guarded,
                "{k:?} is edited by the field but the shortcut guard lets it through"
            );
            focus::clear();
        }
    }

    #[test]
    fn autofocus_makes_a_field_typable_without_a_tap() {
        reset_layout_runtime();
        focus::clear();
        let value = signal(String::new());
        let mut input = Input::new(
            value.clone(),
            LayoutStyle::new().width(200.0).height(20.0),
            || TextStyle::new(14.0, Color::BLACK),
        )
        .unwrap()
        .autofocus();
        assert!(input.focused(), "the field holds focus from construction");
        input.on_event(&key(Key::Char('x')));
        assert_eq!(value.get(), "x", "and the very first keystroke is text");

        // Without it, the same field ignores the keystroke — which is the default a form wants.
        reset_layout_runtime();
        focus::clear();
        let untouched = signal(String::new());
        let mut plain = Input::new(
            untouched.clone(),
            LayoutStyle::new().width(200.0).height(20.0),
            || TextStyle::new(14.0, Color::BLACK),
        )
        .unwrap();
        assert!(!plain.focused());
        plain.on_event(&key(Key::Char('x')));
        assert_eq!(untouched.get(), "");
    }

    #[test]
    fn typing_inserts_at_caret() {
        let (mut input, value) = focused_input("");
        for c in "hi".chars() {
            input.on_event(&key(Key::Char(c)));
        }
        assert_eq!(value.get(), "hi");
        assert_eq!(input.caret.get(), 2);
    }

    #[test]
    fn backspace_and_arrows_edit_mid_string() {
        let (mut input, value) = focused_input("abc");
        // Caret starts at end (3). Left twice → between a and b (1).
        input.on_event(&key(Key::Named(NamedKey::ArrowLeft)));
        input.on_event(&key(Key::Named(NamedKey::ArrowLeft)));
        assert_eq!(input.caret.get(), 1);
        // Backspace removes 'a'.
        input.on_event(&key(Key::Named(NamedKey::Backspace)));
        assert_eq!(value.get(), "bc");
        assert_eq!(input.caret.get(), 0);
        // Insert at start.
        input.on_event(&key(Key::Char('X')));
        assert_eq!(value.get(), "Xbc");
    }

    #[test]
    fn keys_ignored_when_not_focused() {
        let (mut input, value) = focused_input("a");
        focus::clear();
        let r = input.on_event(&key(Key::Char('z')));
        assert_eq!(r, EventResult::Ignored);
        assert_eq!(value.get(), "a", "an unfocused input must not edit");
    }

    #[test]
    fn a_masked_field_hides_the_text_without_changing_it() {
        let (mut input, value) = focused_input("");
        input = input.secret();
        for c in ['h', 'u', 'n', 't', 'e', 'r'] {
            input.on_event(&key(Key::Char(c)));
        }
        assert_eq!(
            value.get(),
            "hunter",
            "the bound signal keeps the real text, which is what a submit handler reads"
        );
        assert_eq!(
            input.shown(&value.get()),
            "••••••",
            "and the screen does not"
        );

        // One mask character per character, not per byte: a multi-byte password must not leak its length in
        // bytes, and the caret is measured against this string.
        assert_eq!(input.shown("mañana"), "••••••");
        assert_eq!(input.shown(""), "");
    }

    #[test]
    fn an_unmasked_field_is_unchanged() {
        let (input, value) = focused_input("plain");
        assert_eq!(input.shown(&value.get()), "plain");
    }

    #[test]
    fn tap_focuses_and_ctrl_chord_is_ignored() {
        let (mut input, value) = focused_input("hi");
        focus::clear();
        let r = input.on_event(&Event::PointerPressed {
            x: 10.0,
            y: 5.0,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        });
        assert_eq!(r, EventResult::Handled);
        assert!(
            focus::is_focused(input.id),
            "a tap inside focuses the input"
        );
        // Ctrl+V is a shortcut, not text: ignored, value unchanged.
        let paste = Event::KeyPressed {
            key: Key::Char('v'),
            modifiers: ModifiersState {
                is_ctrl: true,
                ..Default::default()
            },
        };
        assert_eq!(input.on_event(&paste), EventResult::Ignored);
        assert_eq!(value.get(), "hi");
    }
}
