use std::rc::Rc;
use std::sync::Arc;

use geometry_core::Rect;
use layout_core::{LayoutError, LayoutStyle};
use platform_core::{Event, Key, ModifiersState, NamedKey, PointerButton};
use reactive_core::{Effect, RwSignal, effect, signal};
use renderer_core::{Color, Paint, RectStyle, ShapeStyle, TextStyle};
use ui_tree::{Component, EventResult, RenderNode};

use crate::context::{mark_dirty, new_measured_leaf};
use crate::focus::{self, FocusId};
use crate::impl_leaf_widget;
use crate::layout_leaf::LayoutLeaf;

/// Width of the caret, in logical px.
const CARET_WIDTH: f32 = 1.5;
/// A width so large the shaper never soft-wraps: only an explicit `\n` breaks a line (code-editor behavior),
/// which keeps caret/line math exact — a line's visual row is its logical row.
const NO_WRAP_WIDTH: f32 = 1.0e6;
/// Inserted by the Tab key (soft tabs).
const TAB_INSERT: &str = "    ";

/// A multi-line editable text area bound to a `RwSignal<String>` — the multi-line sibling of [`Input`](crate::Input).
/// A base primitive: unstyled (wrap it in a `box` for a border/background), keyboard-driven, no soft-wrap (only
/// `\n` breaks lines, so long lines overflow horizontally). It requests focus on tap, positions the caret at
/// the click, edits the bound signal from key events (typing, Enter for a newline, Backspace/Delete joining
/// lines, arrows in all four directions, Home/End, Tab), and draws a caret. Its measured height grows with the
/// line count, so wrapping it in a [`LayoutScrollArea`](crate::LayoutScrollArea) gives a scrolling editor.
/// Selection (`Shift`+arrows, `Ctrl+A`, shift-click) with copy, cut and paste, newlines and all. IME is not
/// yet supported.
pub struct TextArea {
    value: RwSignal<String>,
    // Caret byte offset into `value`. Reactive so a bare caret move re-renders even when the text is unchanged.
    caret: RwSignal<usize>,
    // The other end of a selection, or `None` when there is none. Byte offset like `caret`, and either side
    // of it.
    anchor: RwSignal<Option<usize>>,
    style: Rc<dyn Fn() -> TextStyle>,
    id: FocusId,
    leaf: LayoutLeaf,
    placeholder: String,
    // Re-measures the leaf's height whenever the bound value changes — from a keystroke or a programmatic set
    // (e.g. loading a file) — so the line count drives the layout in both cases. Kept alive for the widget's life.
    _remeasure: Effect,
}

impl TextArea {
    pub fn new(
        value: RwSignal<String>,
        layout_style: LayoutStyle,
        style_fn: impl Fn() -> TextStyle + 'static,
    ) -> Result<Self, LayoutError> {
        let style: Rc<dyn Fn() -> TextStyle> = Rc::new(style_fn);
        // Height is measured from the line count at the current style; width is left to the parent (the field
        // stretches to fill the pane), so a long line overflows to the right rather than widening the layout.
        let measure_value = value.clone();
        let measure_style = Rc::clone(&style);
        let measure = Box::new(move |_max_width: f32| {
            let s = (measure_style)();
            let line_h = crate::text_metrics::line_height(s.font_size);
            let lines = measure_value.with(|t| t.matches('\n').count() + 1);
            (0.0, lines as f32 * line_h)
        });
        let (node, rect) = new_measured_leaf(layout_style.align_self_stretch(), measure)?;
        let caret = value.with(|s| s.len());
        let id = focus::next_id();
        focus::register_with_role(
            id,
            focus::FocusKind::TextEntry,
            node,
            focus::Role::MultilineTextInput,
        );
        let remeasure = {
            let value = value.clone();
            effect(move || {
                // Subscribe to the value (tracked read) without cloning it; re-measure on any change.
                value.with(|_| {});
                mark_dirty(node).ok();
            })
        };
        Ok(Self {
            value,
            caret: signal(caret),
            anchor: signal(None),
            style,
            id,
            leaf: LayoutLeaf { node, rect },
            placeholder: String::new(),
            _remeasure: remeasure,
        })
    }

    /// A muted hint shown while the value is empty.
    pub fn placeholder(mut self, p: impl Into<String>) -> Self {
        self.placeholder = p.into();
        self
    }

    /// Gives keyboard focus to this area (as a tap would), leaving the caret where it was. For programmatic
    /// focus — e.g. a container autofocusing the editor when its tab or window becomes active.
    pub fn request_focus(&self) {
        focus::request(self.id);
    }

    /// Whether this area currently holds keyboard focus.
    pub fn focused(&self) -> bool {
        focus::is_focused(self.id)
    }

    /// A `Copy` [`focus::FocusHandle`] to this area, so a caller that has moved it into a container can still
    /// focus it later (e.g. autofocus on tab activation) without keeping a reference to the area itself.
    pub fn focus_handle(&self) -> focus::FocusHandle {
        focus::handle(self.id)
    }

    fn line_height(&self) -> f32 {
        crate::text_metrics::line_height((self.style)().font_size)
    }

    /// The current caret byte offset, clamped to the text and snapped to a char boundary.
    fn caret_at(&self, text: &str) -> usize {
        let mut c = self.caret.get().min(text.len());
        while c > 0 && !text.is_char_boundary(c) {
            c -= 1;
        }
        c
    }

    /// The selected byte range, low end first, or `None` when nothing is selected.
    fn selection(&self, text: &str) -> Option<(usize, usize)> {
        let caret = self.caret_at(text);
        let mut anchor = self.anchor.get()?.min(text.len());
        while anchor > 0 && !text.is_char_boundary(anchor) {
            anchor -= 1;
        }
        (anchor != caret).then(|| (anchor.min(caret), anchor.max(caret)))
    }

    fn selected_text(&self, text: &str) -> Option<String> {
        self.selection(text)
            .map(|(from, to)| text[from..to].to_string())
    }

    /// Removes the selection from `text` and reports where the caret lands. Every edit runs through this
    /// first, so typing over a selection replaces it.
    fn take_selection(&self, text: &mut String) -> Option<usize> {
        let (from, to) = self.selection(text)?;
        text.replace_range(from..to, "");
        Some(from)
    }

    /// Applies a key while focused, editing the bound signal and/or moving the caret. Returns whether the key
    /// was consumed. On a text change the leaf is marked dirty so the runner re-measures the (possibly new)
    /// line count on the next frame.
    fn edit(&mut self, key: &Key, mods: &ModifiersState, style: &TextStyle) -> EventResult {
        let mut text = self.value.get();
        let mut caret = self.caret_at(&text);
        let mut changed = false;
        // Where a movement key leaves the anchor: `Shift` keeps (or starts) a selection, anything else drops
        // it. Resolved after the match so each arm can still read the selection it is replacing.
        let anchor = if mods.is_shift {
            Some(self.anchor.get().unwrap_or(caret))
        } else {
            None
        };
        match key {
            Key::Char('a') | Key::Char('A') if mods.is_ctrl || mods.is_meta => {
                self.anchor.set(Some(0));
                self.caret.set(text.len());
                return EventResult::Handled;
            }
            Key::Char('c') | Key::Char('C') if mods.is_ctrl || mods.is_meta => {
                let Some(selected) = self.selected_text(&text) else {
                    return EventResult::Ignored;
                };
                services_core::set_clipboard_text(&selected);
                // The selection survives a copy, as it does everywhere else.
                return EventResult::Handled;
            }
            Key::Char('x') | Key::Char('X') if mods.is_ctrl || mods.is_meta => {
                let Some(selected) = self.selected_text(&text) else {
                    return EventResult::Ignored;
                };
                services_core::set_clipboard_text(&selected);
                caret = self.take_selection(&mut text).unwrap_or(caret);
                changed = true;
            }
            // Paste replaces the selection, newlines and all — an editor is exactly where a multi-line paste
            // belongs.
            Key::Char('v') | Key::Char('V') if mods.is_ctrl || mods.is_meta => {
                let Some(pasted) = services_core::clipboard_text() else {
                    return EventResult::Ignored;
                };
                let had_selection = self.take_selection(&mut text);
                if pasted.is_empty() && had_selection.is_none() {
                    return EventResult::Ignored;
                }
                caret = had_selection.unwrap_or(caret);
                text.insert_str(caret, &pasted);
                caret += pasted.len();
                changed = true;
            }
            // Any other chord (Ctrl/Meta) is a shortcut, not text — leave it for global handlers (save, …).
            Key::Char(_) if mods.is_ctrl || mods.is_meta => return EventResult::Ignored,
            Key::Char(c) if !c.is_control() => {
                caret = self.take_selection(&mut text).unwrap_or(caret);
                text.insert(caret, *c);
                caret += c.len_utf8();
                changed = true;
            }
            Key::Named(NamedKey::Space) => {
                caret = self.take_selection(&mut text).unwrap_or(caret);
                text.insert(caret, ' ');
                caret += 1;
                changed = true;
            }
            Key::Named(NamedKey::Enter) => {
                caret = self.take_selection(&mut text).unwrap_or(caret);
                text.insert(caret, '\n');
                caret += 1;
                changed = true;
            }
            Key::Named(NamedKey::Tab) => {
                text.insert_str(caret, TAB_INSERT);
                caret += TAB_INSERT.len();
                changed = true;
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
                changed = true;
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
                changed = true;
            }
            Key::Named(NamedKey::ArrowLeft) => caret = prev_boundary(&text, caret),
            Key::Named(NamedKey::ArrowRight) => caret = next_boundary(&text, caret),
            Key::Named(NamedKey::ArrowUp) => {
                let line = line_index(&text, caret);
                if line > 0 {
                    let x = caret_x(&text, caret, style);
                    caret = offset_at_line_x(&text, style, line - 1, x);
                } else {
                    caret = 0;
                }
            }
            Key::Named(NamedKey::ArrowDown) => {
                let line = line_index(&text, caret);
                let last = text.matches('\n').count();
                if line < last {
                    let x = caret_x(&text, caret, style);
                    caret = offset_at_line_x(&text, style, line + 1, x);
                } else {
                    caret = text.len();
                }
            }
            Key::Named(NamedKey::Home) => caret = line_bounds(&text, caret).0,
            Key::Named(NamedKey::End) => caret = line_bounds(&text, caret).1,
            Key::Named(NamedKey::Escape) => {
                focus::release(self.id);
                return EventResult::Handled;
            }
            _ => return EventResult::Ignored,
        }
        if changed {
            // Setting the value fires the re-measure effect (registered in `new`), which marks the leaf dirty
            // so the runner re-measures the (possibly new) line count next frame.
            self.value.set(text);
        }
        self.caret.set(caret);
        // An anchor that caught up with the caret is no selection at all.
        self.anchor.set(anchor.filter(|a| *a != caret));
        EventResult::Handled
    }
}

impl Component for TextArea {
    fn view(&self) -> RenderNode {
        let r = self.leaf.rect.get();
        let text = self.value.get();
        let style = (self.style)();
        let line_h = self.line_height();
        // Render at a huge width so the shaper never soft-wraps (only `\n` breaks a line); long lines overflow
        // to the right and are clipped by an ancestor (e.g. the scroll viewport).
        let full = Rect {
            x: 0.0,
            y: 0.0,
            width: NO_WRAP_WIDTH,
            height: r.height.max(line_h),
        };
        let text_node = if text.is_empty() && !self.placeholder.is_empty() {
            let muted = match style.color {
                Paint::Solid(c) => Paint::Solid(c.with_alpha(c.a * 0.5)),
                _ => Paint::Solid(Color::rgba(0.5, 0.5, 0.55, 0.5)),
            };
            let mut ph_style = style.clone();
            ph_style.color = muted;
            RenderNode::text(self.placeholder.clone(), full, ph_style)
        } else {
            RenderNode::text(Arc::<str>::from(text.as_str()), full, style.clone())
        };

        if focus::is_focused(self.id) {
            let caret = self.caret_at(&text);
            let line = line_index(&text, caret);
            // One rect per line the selection spans: the first from its start to the line's end, the last from
            // the line's start to its end, and every line between them whole. A selection that wraps lines is
            // not one box — drawing it as one would paint over the margin the text does not occupy.
            let highlight = self.selection(&text).map(|(from, to)| {
                let (first, last) = (line_index(&text, from), line_index(&text, to));
                let mut bands = Vec::with_capacity(last - first + 1);
                for line in first..=last {
                    let (start, end) = nth_line_bounds(&text, line);
                    let x0 = if line == first {
                        caret_x(&text, from, &style)
                    } else {
                        caret_x(&text, start, &style)
                    };
                    let x1 = if line == last {
                        caret_x(&text, to, &style)
                    } else {
                        // An empty line still shows a sliver, so a selection running through it is continuous
                        // rather than a gap the eye reads as the selection having ended.
                        caret_x(&text, end, &style).max(x0 + line_h * 0.35)
                    };
                    let fill = match style.color {
                        Paint::Solid(c) => c.with_alpha(0.25),
                        _ => Color::rgba(0.4, 0.6, 0.9, 0.3),
                    };
                    bands.push(RenderNode::rect(
                        Rect {
                            x: x0,
                            y: line as f32 * line_h,
                            width: (x1 - x0).max(1.0),
                            height: line_h,
                        },
                        RectStyle::default().with_fill(Paint::Solid(fill)),
                    ));
                }
                RenderNode::group(bands)
            });
            let x = caret_x(&text, caret, &style);
            let caret_rect = Rect {
                x,
                y: line as f32 * line_h,
                width: CARET_WIDTH,
                height: line_h,
            };
            let caret_node =
                RenderNode::rect(caret_rect, RectStyle::default().with_fill(style.color));
            let layers = match highlight {
                Some(highlight) => vec![highlight, text_node, caret_node],
                None => vec![text_node, caret_node],
            };
            self.leaf.at_layout_position(RenderNode::group(layers))
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
                    let style = (self.style)();
                    let line_h = crate::text_metrics::line_height(style.font_size);
                    // Read the text out (borrow released) before setting the caret: `set` inside a `with`
                    // closure would re-borrow the reactive runtime.
                    let text = self.value.get();
                    let local_y = (*y as f32 - rect.y).max(0.0);
                    let local_x = (*x as f32 - rect.x).max(0.0);
                    let last = text.matches('\n').count();
                    let line = ((local_y / line_h).floor() as usize).min(last);
                    let at = offset_at_line_x(&text, &style, line, local_x);
                    // Shift-click extends from wherever the selection already starts, which is what every
                    // editor does and the only pointer gesture available until a drag can be tracked.
                    if crate::keyboard::modifiers().is_shift {
                        let from = self.anchor.get().unwrap_or_else(|| self.caret.get());
                        self.anchor.set(Some(from));
                    } else {
                        self.anchor.set(None);
                    }
                    self.caret.set(at);
                    EventResult::Handled
                } else {
                    EventResult::Ignored
                }
            }
            Event::KeyPressed { key, modifiers } if focus::is_focused(self.id) => {
                let style = (self.style)();
                self.edit(key, modifiers, &style)
            }
            _ => EventResult::Ignored,
        }
    }

    fn debug_name(&self) -> &'static str {
        "TextArea"
    }
}

impl Drop for TextArea {
    fn drop(&mut self) {
        focus::unregister(self.id);
    }
}

impl_leaf_widget!(TextArea);

/// Byte range `[start, end)` of the line containing `caret` (bounded by the surrounding `\n`s or the text ends).
fn line_bounds(text: &str, caret: usize) -> (usize, usize) {
    let caret = caret.min(text.len());
    let start = text[..caret].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let end = text[caret..]
        .find('\n')
        .map(|i| caret + i)
        .unwrap_or(text.len());
    (start, end)
}

/// Zero-based visual/logical line of `caret` (they are the same with no soft-wrap).
fn line_index(text: &str, caret: usize) -> usize {
    text[..caret.min(text.len())].matches('\n').count()
}

/// Byte range `[start, end)` of the `n`th line (clamped to the last line).
fn nth_line_bounds(text: &str, n: usize) -> (usize, usize) {
    let mut start = 0;
    for _ in 0..n {
        match text[start..].find('\n') {
            Some(i) => start += i + 1,
            None => return (text.len(), text.len()),
        }
    }
    let end = text[start..]
        .find('\n')
        .map(|i| start + i)
        .unwrap_or(text.len());
    (start, end)
}

/// Pixel x of the caret within its line (the advance of the line's prefix up to `caret`).
fn caret_x(text: &str, caret: usize, style: &TextStyle) -> f32 {
    let (start, _) = line_bounds(text, caret);
    crate::text_metrics::measure_text(
        &text[start..caret.min(text.len())],
        None,
        NO_WRAP_WIDTH,
        style,
    )
    .0
}

/// The byte offset within line `n` whose caret x is closest to `x` — used for click-to-position and vertical
/// arrow moves (keeping the column).
fn offset_at_line_x(text: &str, style: &TextStyle, n: usize, x: f32) -> usize {
    let (start, end) = nth_line_bounds(text, n);
    let line = &text[start..end];
    let mut best = start;
    let mut best_dx = f32::MAX;
    let mut idx = 0;
    loop {
        let w = crate::text_metrics::measure_text(&line[..idx], None, NO_WRAP_WIDTH, style).0;
        let dx = (w - x).abs();
        if dx < best_dx {
            best_dx = dx;
            best = start + idx;
        }
        if idx >= line.len() {
            break;
        }
        idx = next_boundary(line, idx);
    }
    best
}

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
    use super::*;
    use crate::context::{compute_layout, new_container, reset_layout_runtime};
    use crate::layout_item::LayoutItem;
    use layout_core::AvailableSpace;
    use renderer_core::Color;

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
    fn typing_over_a_selection_replaces_it() {
        let (mut area, value) = focused("one\ntwo");
        area.on_event(&chord(Key::Char('a')));
        area.on_event(&key(Key::Char('x')));
        assert_eq!(value.get(), "x");
    }

    /// The case the notebook is: a selection that runs across a line break, cut whole.
    #[test]
    fn a_selection_across_lines_cuts_whole() {
        let (mut area, value) = focused("one\ntwo\nthree");
        area.on_event(&chord(Key::Char('a')));
        area.on_event(&chord(Key::Char('x')));
        assert_eq!(value.get(), "");
    }

    #[test]
    fn shift_arrows_grow_a_selection_and_backspace_takes_it() {
        let (mut area, value) = focused("hello");
        area.on_event(&shifted(Key::Named(NamedKey::ArrowLeft)));
        area.on_event(&shifted(Key::Named(NamedKey::ArrowLeft)));
        assert_eq!(area.selection("hello"), Some((3, 5)));
        area.on_event(&key(Key::Named(NamedKey::Backspace)));
        assert_eq!(value.get(), "hel");
    }

    /// Enter with a selection replaces it with the break, rather than pushing the selected text down a line.
    #[test]
    fn enter_replaces_a_selection() {
        let (mut area, value) = focused("abcd");
        area.on_event(&chord(Key::Char('a')));
        area.on_event(&key(Key::Named(NamedKey::Enter)));
        assert_eq!(value.get(), "\n");
    }

    #[test]
    fn copy_leaves_the_text_and_the_selection_alone() {
        let (mut area, value) = focused("one\ntwo");
        area.on_event(&chord(Key::Char('a')));
        assert_eq!(area.on_event(&chord(Key::Char('c'))), EventResult::Handled);
        assert_eq!(value.get(), "one\ntwo");
        assert_eq!(area.selection("one\ntwo"), Some((0, 7)));
    }
    fn focused(initial: &str) -> (TextArea, RwSignal<String>) {
        reset_layout_runtime();
        let value = signal(initial.to_string());
        let area = TextArea::new(value.clone(), LayoutStyle::new().width(400.0), || {
            TextStyle::new(14.0, Color::BLACK)
        })
        .unwrap();
        let root = new_container(
            LayoutStyle::new().flex_column().width(400.0).height(400.0),
            &[area.layout_node()],
        )
        .unwrap();
        compute_layout(
            root,
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(400.0),
        )
        .unwrap();
        focus::request(area.id);
        (area, value)
    }

    /// The multi-line twin of `Input`'s guard test: this editor takes Enter and the vertical arrows as text,
    /// so a global shortcut on any of them must stand aside while the caret is here.
    #[test]
    fn the_shortcut_guard_covers_every_key_this_editor_edits() {
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
            NamedKey::F5,
        ];
        let keys: Vec<Key> = std::iter::once(Key::Char('x'))
            .chain(named.into_iter().map(Key::Named))
            .collect();
        let style = TextStyle::new(14.0, Color::BLACK);
        for k in keys {
            let (mut area, _value) = focused("one\ntwo");
            area.caret.set(5);
            // Asked first, as dispatch does — Escape answers by giving up the focus the guard reads.
            let guarded = focus::text_entry_takes_key(&k, plain);
            let edited = area.edit(&k, &plain, &style) == EventResult::Handled;
            assert!(
                !edited || guarded,
                "{k:?} is edited by the editor but the shortcut guard lets it through"
            );
            focus::clear();
        }
    }

    #[test]
    fn enter_inserts_newline_and_typing_continues_on_new_line() {
        let (mut area, value) = focused("ab");
        area.on_event(&key(Key::Named(NamedKey::Enter)));
        area.on_event(&key(Key::Char('c')));
        assert_eq!(value.get(), "ab\nc");
        assert_eq!(line_index(&value.get(), area.caret.get()), 1);
    }

    #[test]
    fn backspace_at_line_start_joins_lines() {
        let (mut area, value) = focused("ab\ncd");
        // Caret starts at end (line 1). Home → line start (byte 3), Backspace removes the newline.
        area.on_event(&key(Key::Named(NamedKey::Home)));
        area.on_event(&key(Key::Named(NamedKey::Backspace)));
        assert_eq!(value.get(), "abcd");
    }

    #[test]
    fn arrow_up_down_moves_between_lines() {
        let (mut area, value) = focused("aaaa\nbb");
        // Caret at end of "bb" (line 1). Up → line 0, keeping column; Down → back to line 1.
        area.on_event(&key(Key::Named(NamedKey::ArrowUp)));
        assert_eq!(line_index(&value.get(), area.caret.get()), 0);
        area.on_event(&key(Key::Named(NamedKey::ArrowDown)));
        assert_eq!(line_index(&value.get(), area.caret.get()), 1);
    }

    #[test]
    fn click_focuses_and_positions_caret_without_reborrow() {
        use platform_core::{PointerButton, PointerSource};
        let (mut area, _value) = focused("hello\nworld");
        focus::clear();
        // A press inside must focus the field and move the caret without re-borrowing the reactive runtime
        // (the caret set must not run inside a `value.with` closure).
        let r = area.leaf.rect.get();
        let handled = area.on_event(&Event::PointerPressed {
            x: (r.x + 5.0) as f64,
            y: (r.y + 2.0) as f64,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        });
        assert_eq!(handled, EventResult::Handled);
        assert!(
            focus::is_focused(area.id),
            "a press inside focuses the area"
        );
    }

    #[test]
    fn ctrl_chord_is_ignored_as_shortcut() {
        let (mut area, value) = focused("hi");
        let save = Event::KeyPressed {
            key: Key::Char('s'),
            modifiers: ModifiersState {
                is_ctrl: true,
                ..Default::default()
            },
        };
        assert_eq!(area.on_event(&save), EventResult::Ignored);
        assert_eq!(value.get(), "hi");
    }
}
