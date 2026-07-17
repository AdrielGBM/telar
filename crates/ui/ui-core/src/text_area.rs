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
/// Selection, clipboard, and IME are not yet supported (a single-caret MVP, like `Input`).
pub struct TextArea {
    value: RwSignal<String>,
    // Caret byte offset into `value`. Reactive so a bare caret move re-renders even when the text is unchanged.
    caret: RwSignal<usize>,
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
            let line_h = s.font_size * renderer_text::LINE_HEIGHT_FACTOR;
            let lines = measure_value.with(|t| t.matches('\n').count() + 1);
            (0.0, lines as f32 * line_h)
        });
        let (node, rect) = new_measured_leaf(layout_style.align_self_stretch(), measure)?;
        let caret = value.with(|s| s.len());
        let id = focus::next_id();
        focus::register(id);
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

    fn line_height(&self) -> f32 {
        (self.style)().font_size * renderer_text::LINE_HEIGHT_FACTOR
    }

    /// The current caret byte offset, clamped to the text and snapped to a char boundary.
    fn caret_at(&self, text: &str) -> usize {
        let mut c = self.caret.get().min(text.len());
        while c > 0 && !text.is_char_boundary(c) {
            c -= 1;
        }
        c
    }

    /// Applies a key while focused, editing the bound signal and/or moving the caret. Returns whether the key
    /// was consumed. On a text change the leaf is marked dirty so the runner re-measures the (possibly new)
    /// line count on the next frame.
    fn edit(&mut self, key: &Key, mods: &ModifiersState, style: &TextStyle) -> EventResult {
        let mut text = self.value.get();
        let mut caret = self.caret_at(&text);
        let mut changed = false;
        match key {
            // A chord (Ctrl/Meta) is a shortcut, not text — leave it for global handlers (save, copy/paste TBD).
            Key::Char(_) if mods.is_ctrl || mods.is_meta => return EventResult::Ignored,
            Key::Char(c) if !c.is_control() => {
                text.insert(caret, *c);
                caret += c.len_utf8();
                changed = true;
            }
            Key::Named(NamedKey::Space) => {
                text.insert(caret, ' ');
                caret += 1;
                changed = true;
            }
            Key::Named(NamedKey::Enter) => {
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
                if caret == 0 {
                    return EventResult::Ignored;
                }
                let prev = prev_boundary(&text, caret);
                text.replace_range(prev..caret, "");
                caret = prev;
                changed = true;
            }
            Key::Named(NamedKey::Delete) => {
                if caret >= text.len() {
                    return EventResult::Ignored;
                }
                let next = next_boundary(&text, caret);
                text.replace_range(caret..next, "");
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
            let muted = match style.paint {
                Paint::Solid(c) => Paint::Solid(c.with_alpha(c.a * 0.5)),
                _ => Paint::Solid(Color::rgba(0.5, 0.5, 0.55, 0.5)),
            };
            let mut ph_style = style;
            ph_style.paint = muted;
            RenderNode::text(self.placeholder.clone(), full, ph_style)
        } else {
            RenderNode::text(Arc::<str>::from(text.as_str()), full, style)
        };

        if focus::is_focused(self.id) {
            let caret = self.caret_at(&text);
            let line = line_index(&text, caret);
            let x = caret_x(&text, caret, &style);
            let caret_rect = Rect {
                x,
                y: line as f32 * line_h,
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
                    focus::request(self.id);
                    let style = (self.style)();
                    let line_h = style.font_size * renderer_text::LINE_HEIGHT_FACTOR;
                    // Read the text out (borrow released) before setting the caret: `set` inside a `with`
                    // closure would re-borrow the reactive runtime.
                    let text = self.value.get();
                    let local_y = (*y as f32 - rect.y).max(0.0);
                    let local_x = (*x as f32 - rect.x).max(0.0);
                    let last = text.matches('\n').count();
                    let line = ((local_y / line_h).floor() as usize).min(last);
                    self.caret.set(offset_at_line_x(&text, &style, line, local_x));
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
    let end = text[start..].find('\n').map(|i| start + i).unwrap_or(text.len());
    (start, end)
}

/// Pixel x of the caret within its line (the advance of the line's prefix up to `caret`).
fn caret_x(text: &str, caret: usize, style: &TextStyle) -> f32 {
    let (start, _) = line_bounds(text, caret);
    renderer_text::measure_text(&text[start..caret.min(text.len())], NO_WRAP_WIDTH, style).0
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
        let w = renderer_text::measure_text(&line[..idx], NO_WRAP_WIDTH, style).0;
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

    fn focused(initial: &str) -> (TextArea, RwSignal<String>) {
        reset_layout_runtime();
        let value = signal(initial.to_string());
        let area = TextArea::new(
            value.clone(),
            LayoutStyle::new().width(400.0),
            || TextStyle::new(14.0, Color::BLACK),
        )
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
        assert!(focus::is_focused(area.id), "a press inside focuses the area");
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
