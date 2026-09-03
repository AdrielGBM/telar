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

/// A single-line editable text field bound to a `RwSignal<String>`. A base primitive: unstyled (no
/// border or background — wrap it in a `box` for the look) and keyboard-driven. It requests focus on
/// tap and, while focused, edits the bound signal from key events, drawing a caret at the insertion
/// point. Selection (`Shift`+arrows/Home/End, `Ctrl+A`) with copy, cut and paste; IME composition is not yet
/// supported. Drag-to-select waits on click-to-position, which this field does not have either.
pub struct Input {
    value: RwSignal<String>,
    // Caret byte offset into `value`. Reactive so a bare caret move (arrows/home/end) re-renders even
    // when the text is unchanged; always re-snapped to a char boundary in case the signal changed elsewhere.
    caret: RwSignal<usize>,
    // The other end of a selection, or `None` when there is none. Byte offset like `caret`, and either side
    // of it: a selection extended leftwards has its anchor after its caret.
    anchor: RwSignal<Option<usize>>,
    style: Rc<dyn Fn() -> TextStyle>,
    id: FocusId,
    leaf: LayoutLeaf,
    on_submit: Option<Box<dyn Fn()>>,
    on_cancel: Option<Box<dyn Fn()>>,
    // Hint shown (muted) while the value is empty. Rendered in place of the text so the field stays a live,
    // tappable `Input` even when empty — a separate placeholder widget swapped in would not take focus.
    placeholder: String,
    // Character drawn in place of every character of the value. Affects rendering only: the bound signal, the
    // caret offsets and every edit still work on the real text.
    mask: Option<char>,
    blink: Blink,
    // Keeps the blink running while the field holds the keyboard, and stops it when it does not.
    _blinking: Effect,
}

impl Input {
    /// A single-line field, and whether the keyboard is in it.
    ///
    /// A target that draws its own caret needs neither; one that hands the box to a document needs both, so
    /// the document's own focus lands on the field a person clicked into rather than beside it.
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

    /// A field styled by what the tree above it declared, amended by whatever it says for itself — the
    /// counterpart of [`Text::declaring`](crate::Text::declaring), and what keeps a field's text the same
    /// size as the labels beside it.
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
        // Join the tab order so Tab/Shift-Tab can reach this field, as the kind that takes keys as text — so
        // an app-level shortcut table can stand aside while the caret is here.
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

    /// Runs when Escape is pressed while focused, just before the field hands the keyboard back.
    ///
    /// **The other half of [`on_submit`](Self::on_submit).** A field that can be committed but not abandoned
    /// is half a contract, and the missing half is the one a caller cannot write for itself: Escape is a key a
    /// focused field eats, so an application watching from outside sees the keyboard leave and has no way to
    /// tell «they gave up» from «they clicked somewhere else» — opposite answers wherever losing focus commits.
    pub fn on_cancel(mut self, f: impl Fn() + 'static) -> Self {
        self.on_cancel = Some(Box::new(f));
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

    /// The id this field holds in the tab order.
    ///
    /// For the caller that has to answer «who has the keyboard» about a field it did not build — a tab that
    /// turns into one, a cell edited in place — and cannot ask [`focus::current`] instead: that says who holds
    /// it now, which is the same answer for every field on the surface. `focus_id:$sig` in a `[view]` is this,
    /// mirrored into a signal and withdrawn when the field goes.
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

    /// The selected byte range, low end first, or `None` when nothing is selected. An anchor sitting on the
    /// caret is not a selection — it is where one would start from.
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

    /// Removes the selection from `text` and reports where the caret lands, or `None` when there was none.
    /// Every edit runs through this first: typing over a selection replaces it, which is the behaviour that
    /// makes a selection worth having.
    fn take_selection(&self, text: &mut String) -> Option<usize> {
        let (from, to) = self.selection(text)?;
        text.replace_range(from..to, "");
        Some(from)
    }

    /// Applies a key while focused, editing the bound signal and/or moving the caret. Returns whether the
    /// key was consumed.
    fn edit(&mut self, key: &Key, mods: &ModifiersState) -> EventResult {
        // Lit again from the top, before the key is even read: a caret that blinked out under the hand is missing at the one moment somebody is looking for it.
        self.blink.wake();
        let mut text = self.value.get();
        let mut caret = self.caret_at(&text);
        let chord = mods.is_ctrl || mods.is_meta;
        // Where a movement key leaves the anchor: `Shift` keeps (or starts) a selection, anything else drops
        // it. Set after the match so each arm can still read the selection it is replacing.
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
                // The selection survives a copy, as it does everywhere else.
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
                // A single-line field takes the first line: a multi-line paste would otherwise put a `\n` in a
                // value nothing can render, and every field is bound to a signal something else reads.
                let pasted = pasted.lines().next().unwrap_or_default().to_string();
                let had_selection = self.take_selection(&mut text);
                if pasted.is_empty() && had_selection.is_none() {
                    return EventResult::Ignored;
                }
                caret = had_selection.unwrap_or(caret);
                text.insert_str(caret, &pasted);
                caret += pasted.len();
            }
            // Any other chord is a shortcut, not text — leave it for global handlers.
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
            // Backspace and Delete take the selection when there is one, and one character when there is not.
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
            // An unshifted arrow with a selection collapses to its edge rather than moving from the caret:
            // pressing Left with three characters selected puts the caret before them, not inside them.
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
                // Said before the keyboard goes back, so a caller watching focus sees the giving-up first and reads what follows as the consequence rather than as a second event.
                if let Some(cb) = &self.on_cancel {
                    cb();
                }
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
        let edited = self.value.with(|s| s != &text);
        if edited {
            self.value.set(text);
        }
        self.caret.set(caret);
        // An anchor that caught up with the caret is no selection at all, and keeping it would make the next
        // unshifted arrow collapse to a range of nothing.
        //
        // An edit drops it outright: the rule above is about *movement*, and `Shift`+`M` is not a movement.
        // Kept, the anchor sat where the caret was before the letter went in, so the next keystroke read the
        // letter just typed as a selection and replaced it — a name came out with every capital missing but
        // the last.
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
        // Empty value → draw the muted placeholder in the text's place (the field itself stays live: the
        // caret and hit-test still work, so it's tappable/typable from empty).
        let text_node = if text.is_empty() && !self.placeholder.is_empty() {
            let mut ph_style = style.clone();
            ph_style.color = style.color.faded(0.5);
            RenderNode::text(self.placeholder.clone(), full, ph_style)
        } else {
            RenderNode::text(self.shown(&text), full, style.clone())
        };

        // The caret is drawn only while focused; reading `is_focused` subscribes this view to focus moves.
        if focus::is_focused(self.id) {
            let caret = self.caret_at(&text);
            // Where the shaper is about to put the first glyph. Everything drawn beside the letters — the caret, the selection — is placed from here rather than from the edge of the box, or a field that inherited a centred alignment draws its text in the middle and its caret at the left.
            let line = self.shown(&text);
            let (line_w, _) = crate::text_metrics::measure_text(&line, None, 1.0e6, &style);
            let origin = align_origin(style.text_align, full.width, line_w);
            // The selection paints *behind* the text, in the ink at low alpha rather than a token of its own:
            // a field is unstyled by design and has no palette to reach for, and the ink is the one colour it
            // is already guaranteed to contrast with.
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
            // Measured against what is *drawn*: a mask character is not the width of the character it hides,
            // so measuring the real prefix would put the caret somewhere the text is not.
            let prefix = self.shown(&text[..caret]);
            let (prefix_w, _) = crate::text_metrics::measure_text(&prefix, None, 1.0e6, &style);
            let line_h = crate::text_metrics::line_box(&style);
            let caret_rect = Rect {
                x: origin + prefix_w,
                y: 0.0,
                width: CARET_WIDTH,
                height: line_h,
            };
            // Read here rather than anywhere else: this is what subscribes the field's own view to the blink, so the caret is the only thing on the surface that redraws on its account.
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
                    // MVP: land the caret at the end. Click-to-position (measuring per glyph) is a follow-up,
                    // and drag-to-select waits on it — there is no x-to-offset mapping to drag along yet.
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
    use renderer_core::{Color, DrawCommand, LineHeight, Paint, TextAlign};
    use std::time::Duration;

    use super::*;
    use crate::context::{compute_layout, new_container};
    use crate::layout_item::LayoutItem;

    fn key(k: Key) -> Event {
        Event::KeyPressed {
            key: k,
            modifiers: ModifiersState::default(),
        }
    }

    fn chord(k: Key) -> Event {
        Event::KeyPressed {
            key: k,
            modifiers: ModifiersState {
                is_ctrl: true,
                ..ModifiersState::default()
            },
        }
    }

    fn shifted(k: Key) -> Event {
        Event::KeyPressed {
            key: k,
            modifiers: ModifiersState {
                is_shift: true,
                ..ModifiersState::default()
            },
        }
    }

    #[test]
    fn shift_arrows_grow_a_selection_and_a_plain_one_drops_it() {
        let (mut input, _) = focused_input("hello");
        input.on_event(&shifted(Key::Named(NamedKey::ArrowLeft)));
        input.on_event(&shifted(Key::Named(NamedKey::ArrowLeft)));
        assert_eq!(input.selection("hello"), Some((3, 5)), "two chars selected");
        input.on_event(&key(Key::Named(NamedKey::ArrowRight)));
        assert_eq!(input.selection("hello"), None, "a plain arrow drops it");
    }

    /// The behaviour that makes a selection worth having: what you type lands *instead of* it.
    #[test]
    fn typing_over_a_selection_replaces_it() {
        let (mut input, value) = focused_input("hello");
        input.on_event(&chord(Key::Char('a')));
        input.on_event(&key(Key::Char('x')));
        assert_eq!(value.get(), "x");
    }

    #[test]
    fn backspace_takes_the_selection_rather_than_one_character() {
        let (mut input, value) = focused_input("hello");
        input.on_event(&shifted(Key::Named(NamedKey::ArrowLeft)));
        input.on_event(&shifted(Key::Named(NamedKey::ArrowLeft)));
        input.on_event(&key(Key::Named(NamedKey::Backspace)));
        assert_eq!(value.get(), "hel");
    }

    /// An unshifted arrow with a selection collapses to its edge — pressing Left with three characters
    /// selected puts the caret before them, not one step in from wherever the caret happened to be.
    #[test]
    fn a_plain_arrow_collapses_to_the_selection_edge() {
        let (mut input, _) = focused_input("hello");
        input.on_event(&chord(Key::Char('a')));
        input.on_event(&key(Key::Named(NamedKey::ArrowLeft)));
        assert_eq!(input.caret.get(), 0, "collapsed to the low edge");
    }

    #[test]
    fn cut_removes_the_selection_and_copy_leaves_it() {
        let (mut input, value) = focused_input("hello");
        input.on_event(&chord(Key::Char('a')));
        input.on_event(&chord(Key::Char('c')));
        assert_eq!(value.get(), "hello", "copy leaves the text alone");
        assert_eq!(input.selection("hello"), Some((0, 5)), "and the selection");
        input.on_event(&chord(Key::Char('x')));
        assert_eq!(value.get(), "", "cut takes it");
    }

    /// Copy and cut with nothing selected report `Ignored`, so a global shortcut table still sees the chord
    /// instead of it being swallowed by a field that did nothing with it.
    #[test]
    fn copy_without_a_selection_is_not_consumed() {
        let (mut input, _) = focused_input("hello");
        assert_eq!(input.on_event(&chord(Key::Char('c'))), EventResult::Ignored);
    }
    // Builds a focused, laid-out input bound to `initial` and returns it plus its value signal.
    fn focused_input(initial: &str) -> (Input, RwSignal<String>) {
        reset_layout_runtime();
        let value = signal(initial.to_string());
        let input = Input::new(value, LayoutStyle::new().width(200.0).height(20.0), || {
            TextStyle::new(14.0, Color::BLACK)
        })
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

    /// A field styled to the given alignment, focused, with its rect laid out.
    fn aligned_input(initial: &str, align: TextAlign) -> Input {
        reset_layout_runtime();
        let value = signal(initial.to_string());
        let input = Input::new(
            value,
            LayoutStyle::new().width(200.0).height(20.0),
            move || {
                let mut style = TextStyle::new(14.0, Color::BLACK);
                style.text_align = align;
                style
            },
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
        input
    }

    /// Where the caret was drawn, and how lit it was.
    fn caret_of(input: &Input) -> (Rect, f32) {
        fn walk(node: &RenderNode, found: &mut Option<(Rect, f32)>) {
            match node {
                RenderNode::Primitive(DrawCommand::Rect { rect, style })
                    if rect.width == CARET_WIDTH =>
                {
                    let lit = match style.fill {
                        Some(Paint::Solid(color)) => color.a,
                        _ => 0.0,
                    };
                    *found = Some((*rect, lit));
                }
                RenderNode::Group { children }
                | RenderNode::Transform { children, .. }
                | RenderNode::Clip { children, .. }
                | RenderNode::Layer { children, .. } => {
                    for child in children.iter() {
                        walk(child, found);
                    }
                }
                _ => {}
            }
        }
        let view = input.view();
        let mut found = None;
        walk(&view, &mut found);
        found.expect("a focused field draws its caret")
    }

    /// **The caret goes where the letters went.** A field inherits its alignment from the region around it —
    /// a centred column of chrome hands one down — and the text is placed by the shaper while the caret is
    /// placed here, so the two have to answer the same question. They did not: the letters sat in the middle
    /// of the box and the caret against its left edge, a whole field's width away from the text it was in.
    #[test]
    fn the_caret_follows_the_alignment_the_text_was_drawn_with() {
        let left = caret_of(&aligned_input("hola", TextAlign::Start)).0.x;
        let middle = caret_of(&aligned_input("hola", TextAlign::Center)).0.x;
        let right = caret_of(&aligned_input("hola", TextAlign::End)).0.x;

        assert!(
            middle > left && right > middle,
            "el cursor no sigue la alineación: {left} / {middle} / {right}"
        );
        // At the end of the text in a box 200 wide, so the caret lands on the far edge whatever the face measures.
        assert!(
            (right - 200.0).abs() < 1.0,
            "alineado a la derecha el cursor va al borde, no a {right}"
        );
    }

    /// **A caret that does not blink is a caret nobody finds** — a hairline of ink that may be sitting in an
    /// empty field, and the eye goes to what changes.
    #[test]
    fn the_caret_blinks_while_the_field_holds_the_keyboard() {
        let input = aligned_input("hola", TextAlign::Start);
        motion_core::reset();
        input.blink.follow(true);

        let start = web_time::Instant::now();
        motion_core::tick(start);
        let lit = caret_of(&input).1;
        motion_core::tick(start + Duration::from_millis(700));
        let out = caret_of(&input).1;

        assert!(lit > 0.9, "el cursor empieza encendido, no en {lit}");
        assert!(out < 0.1, "y se apaga a la mitad del ciclo, no en {out}");
    }

    /// And it is lit again by the key, so it is never missing at the moment somebody is looking for it.
    #[test]
    fn typing_lights_the_caret_again() {
        let (mut input, _) = focused_input("hola");
        motion_core::reset();
        input.blink.follow(true);
        let start = web_time::Instant::now();
        motion_core::tick(start);
        motion_core::tick(start + Duration::from_millis(700));
        assert!(caret_of(&input).1 < 0.1, "el cursor debería estar apagado");

        input.on_event(&key(Key::Char('x')));

        assert!(
            caret_of(&input).1 > 0.9,
            "la tecla no volvió a encender el cursor"
        );
    }

    /// **The caret stands in the line, not over it.** The height came from the face's natural leading while
    /// the text was laid out at whatever the tree declared, so under a `line_height: 1.0` — a pixel face kept
    /// on its own grid — the caret hung below the descenders of the very word it was standing in.
    #[test]
    fn the_caret_is_as_tall_as_the_line_the_text_is_laid_out_on() {
        reset_layout_runtime();
        let value = signal("hola".to_string());
        let input = Input::new(value, LayoutStyle::new().width(200.0).height(20.0), || {
            let mut style = TextStyle::new(14.0, Color::BLACK);
            style.line_height = LineHeight::Times(1.0);
            style
        })
        .unwrap();
        focus::request(input.id);

        let tall = caret_of(&input).0.height;

        assert!(
            (tall - 14.0).abs() < 0.01,
            "una línea de 1.0 sobre 14 mide 14, no {tall}"
        );
        assert!(
            crate::text_metrics::line_height(14.0) > 14.5,
            "la natural es más alta, que es lo que hacía el cursor demasiado largo"
        );
    }

    /// Escape is a key a focused field eats, so an application watching from the outside cannot tell it from
    /// a click elsewhere — and where losing focus commits, those are opposite answers.
    #[test]
    fn escape_says_so_before_it_hands_the_keyboard_back() {
        let (mut input, _) = focused_input("hola");
        let given_up = Rc::new(std::cell::Cell::new(false));
        let said = Rc::clone(&given_up);
        input = input.on_cancel(move || said.set(true));

        input.on_event(&key(Key::Named(NamedKey::Escape)));

        assert!(given_up.get(), "el campo se rindió sin decirlo");
        assert!(!focus::is_focused(input.id), "y soltó el teclado");
    }

    /// The other half of the blink, and the one that decides what the window costs: a sequence registered with
    /// the ticker is a standing request to redraw, so a field nobody is typing into must not be running one.
    #[test]
    fn a_field_without_the_keyboard_asks_for_no_frames() {
        motion_core::reset();
        let input = aligned_input("hola", TextAlign::Start);
        input.blink.follow(false);
        motion_core::tick(web_time::Instant::now());
        assert!(
            !motion_core::has_active(),
            "un campo sin el teclado sigue pidiendo cuadros"
        );
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
        let mut input = Input::new(value, LayoutStyle::new().width(200.0).height(20.0), || {
            TextStyle::new(14.0, Color::BLACK)
        })
        .unwrap()
        .autofocus();
        assert!(
            focus::is_focused(input.id),
            "the field holds focus from construction"
        );
        input.on_event(&key(Key::Char('x')));
        assert_eq!(value.get(), "x", "and the very first keystroke is text");

        // Without it, the same field ignores the keystroke — which is the default a form wants.
        reset_layout_runtime();
        focus::clear();
        let untouched = signal(String::new());
        let mut plain = Input::new(
            untouched,
            LayoutStyle::new().width(200.0).height(20.0),
            || TextStyle::new(14.0, Color::BLACK),
        )
        .unwrap();
        assert!(!focus::is_focused(plain.id));
        plain.on_event(&key(Key::Char('x')));
        assert_eq!(untouched.get(), "");
    }

    /// A capital is typed with `Shift` down, as a keyboard sends it — and `Shift` is the selection modifier,
    /// so the anchor it left behind made the next keystroke replace the letter just typed. A name came out
    /// with every capital missing but the last, which no test that presses `M` where a hand presses
    /// `Shift`+`M` can see.
    #[test]
    fn a_capital_is_not_a_selection() {
        let (mut input, value) = focused_input("");
        for (c, held) in [
            ('L', true),
            ('a', false),
            (' ', false),
            ('C', true),
            ('u', false),
            ('e', false),
            ('v', false),
            ('a', false),
        ] {
            let event = match (c, held) {
                (' ', _) => key(Key::Named(NamedKey::Space)),
                (c, true) => shifted(Key::Char(c)),
                (c, false) => key(Key::Char(c)),
            };
            input.on_event(&event);
        }
        assert_eq!(value.get(), "La Cueva");
    }

    /// `Shift` still selects when the key it is held with is a movement.
    #[test]
    fn a_shifted_arrow_still_selects() {
        let (mut input, value) = focused_input("hola");
        input.on_event(&key(Key::Named(NamedKey::End)));
        for _ in 0..2 {
            input.on_event(&shifted(Key::Named(NamedKey::ArrowLeft)));
        }
        input.on_event(&key(Key::Char('y')));
        assert_eq!(value.get(), "hoy");
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
