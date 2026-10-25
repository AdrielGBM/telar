//! [`Editor`]: what a field of one line and an area of several are both made of — the caret and the side of a wrap it stands on, the selection and the presses and drags that make one, the keys, the clipboard, an input method's composition, the rows the text is drawn in, and keeping the caret in view.
//!
//! [`Input`](crate::Input) is an editor of [`Lines::Single`] and [`TextArea`](crate::TextArea) one of [`Lines::Multi`]; neither does any of this itself, so the two cannot drift apart.

use std::cell::{Cell, RefCell};
use std::ops::Range;
use std::rc::Rc;

use geometry_core::{BorderRadius, Rect};
use layout_core::{AvailableSpace, LayoutError, LayoutStyle, NodeId};
use platform_core::{
    Event, Ime, Key, KeyboardConvention, ModifiersState, NamedKey, PointerButton, PointerSource,
    TextInput, keyboard_convention,
};
use reactive_core::{Effect, RwSignal, effect, signal};
use renderer_core::{Color, LineHeight, RectStyle, Semantics, ShapeStyle, TextStyle, TextWrap};
use ui_tree::{EventResult, RenderNode};

use crate::caret::Blink;
use crate::composition::{self, Composed, Composition, Step};
use crate::context::mark_dirty;
use crate::focus::{self, FocusId};
use crate::input_region::InputHandle;
use crate::layout_leaf::LayoutLeaf;
use crate::surface_context::HomeSurface;
use crate::text_edit::{
    Clicks, Granularity, floor_boundary, line_at, next_boundary, prev_boundary, word_at,
};
use crate::text_place::{CARET_WIDTH, Frame, Hit, Placed, RowCache};
use crate::visual_lines::{Affinity, VisualLines};

/// How much of the field's ink a hint keeps: enough that it still reads at 4.5:1 on a field's surface, which WCAG asks of placeholder text as of any other.
pub(crate) const PLACEHOLDER_OPACITY: f32 = 0.7;

/// How thick an [`Underline`] is drawn, in logical px.
pub(crate) const UNDERLINE_THICKNESS: f32 = 2.0;

/// How wide an [`Underline`] of no characters is drawn: a mark where something was expected and nothing was written.
pub(crate) const POINT_MARK: f32 = 6.0;

/// Inserted by the Tab key in an editor of several lines (soft tabs).
const TAB_INSERT: &str = "    ";

/// What hears an editor's keys before it does, and answers whether it took one.
type KeyHandler = dyn Fn(&Key, ModifiersState) -> bool;

/// A line drawn under a run of the editor's text: a misspelt word, the part of a formula an error is about.
#[derive(Clone, Debug, PartialEq)]
pub struct Underline {
    /// The run, in bytes of the text; an empty one marks the place it starts at.
    pub range: Range<usize>,
    pub color: Color,
}

/// Where an editor's insertion point is, for the code that completes or inserts at it.
///
/// A byte offset into the text. The editor snaps it to a char boundary as it reads it, and reads one past the end of a text that shrank from outside as the end.
#[derive(Clone, Copy)]
pub struct Caret {
    at: RwSignal<usize>,
    anchor: RwSignal<Option<usize>>,
}

impl Caret {
    /// Where it is. Reactive.
    pub fn get(&self) -> usize {
        self.at.get()
    }

    pub fn peek(&self) -> usize {
        self.at.peek()
    }

    /// Moves it to `at`, dropping the selection: what a completion inserted at the caret leaves behind.
    pub fn set(&self, at: usize) {
        self.anchor.set(None);
        self.at.set(at);
    }
}

/// How many lines an editor holds, which decides what Enter, Tab and the vertical arrows do and how the box is sized.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Lines {
    /// One line that never wraps: Enter submits, Tab moves on, and a line longer than the box scrolls sideways under it.
    Single,
    /// Several: Enter breaks the line, the box is as tall as the rows the text is drawn in, and the scroll it sits in follows the caret.
    Multi,
}

/// What the measure, the effects and the view share about how the text is drawn.
pub(crate) struct Shape {
    lines: Lines,
    wrap: Cell<TextWrap>,
    placeholder: RefCell<String>,
    mask: Cell<Option<char>>,
    rows: RowCache,
    // What the box was last measured from: its row count, the width that count was taken at, and the style.
    measured: RefCell<Option<(usize, Option<f32>, TextStyle)>>,
    /// How far the text is scrolled sideways under a box it runs past.
    scroll_x: RwSignal<f32>,
}

impl Shape {
    fn wrap_width(&self, width: f32) -> Option<f32> {
        (self.lines == Lines::Multi && self.wrap.get() == TextWrap::Wrap && width > 0.0)
            .then_some(width)
    }

    /// How a box `width` by `height` frames text in `style`, scrolled `scroll_x` sideways.
    fn frame(&self, width: f32, height: f32, style: &TextStyle, scroll_x: f32) -> Frame {
        let top = match self.lines {
            Lines::Single => line_inset(height, style.font_size),
            Lines::Multi => 0.0,
        };
        Frame {
            wrap: self.wrap_width(width),
            width,
            top,
            scroll_x,
            line_h: crate::text_metrics::line_box(style),
        }
    }

    /// How many rows tall what is drawn is: the text, or the placeholder standing in for it.
    fn shown_rows(&self, text: &str, style: &TextStyle, width: Option<f32>) -> usize {
        if text.is_empty() {
            VisualLines::of(&self.placeholder.borrow(), style, width).len()
        } else {
            self.rows.lines(text, style, width).len()
        }
    }

    /// Forgets the last measurement, so the next change to the text measures again whatever it holds.
    fn changed(&self, node: NodeId) {
        *self.measured.borrow_mut() = None;
        mark_dirty(node).ok();
    }
}

/// A press being dragged: how it extends, and the run the press itself took, which the drag never gives up.
#[derive(Clone, Copy)]
struct Drag {
    granularity: Granularity,
    origin: (usize, usize),
}

/// The editing core of [`Input`](crate::Input) and [`TextArea`](crate::TextArea).
pub(crate) struct Editor {
    pub(crate) value: RwSignal<String>,
    home: HomeSurface,
    // Reactive so a bare caret move re-renders even when the text is unchanged; always re-snapped to a char boundary in case the signal changed elsewhere.
    pub(crate) caret: RwSignal<usize>,
    pub(crate) affinity: RwSignal<Affinity>,
    // Either side of the caret: a selection extended leftwards has its anchor after it.
    pub(crate) anchor: RwSignal<Option<usize>>,
    // Kept across consecutive Up/Down presses, so passing through a short row does not pull the caret to its end for good.
    goal_x: Option<f32>,
    pub(crate) style: Rc<dyn Fn() -> TextStyle>,
    pub(crate) shape: Rc<Shape>,
    pub(crate) id: FocusId,
    pub(crate) leaf: LayoutLeaf,
    pub(crate) blink: Blink,
    pub(crate) composition: Composition,
    drawn_caret: Rc<Cell<Option<TextInput>>>,
    clicks: Clicks,
    drag: Option<Drag>,
    pub(crate) on_submit: Option<Box<dyn Fn()>>,
    pub(crate) on_cancel: Option<Box<dyn Fn()>>,
    pub(crate) on_key: Option<Box<KeyHandler>>,
    pub(crate) underlines: Option<Box<dyn Fn() -> Vec<Underline>>>,
    _remeasure: Option<Effect>,
    _blinking: Effect,
    _follow_caret: Effect,
    _input: InputHandle,
}

impl Editor {
    /// An editor of `lines` bound to `value`, in the style `style` builds once it is handed the cell its node will be in: a measured leaf's style is read before its node exists.
    pub(crate) fn build(
        value: RwSignal<String>,
        layout_style: LayoutStyle,
        lines: Lines,
        style: impl FnOnce(Rc<Cell<Option<NodeId>>>) -> Rc<dyn Fn() -> TextStyle>,
    ) -> Result<Self, LayoutError> {
        let node_cell = Rc::new(Cell::new(None));
        let declared = style(Rc::clone(&node_cell));
        let shape = Rc::new(Shape {
            lines,
            wrap: Cell::new(TextWrap::NoWrap),
            placeholder: RefCell::new(String::new()),
            mask: Cell::new(None),
            rows: RowCache::default(),
            measured: RefCell::new(None),
            scroll_x: signal(0.0),
        });
        // A case that changed a letter's length would put the caret and the selection, which index the value, between the wrong letters.
        let style: Rc<dyn Fn() -> TextStyle> = {
            let shape = Rc::clone(&shape);
            Rc::new(move || {
                declared()
                    .with_text_case(renderer_core::TextCase::AsWritten)
                    .with_text_wrap(shape.wrap.get())
            })
        };
        let caret = signal(value.with(|s| s.len()));
        let affinity = signal(Affinity::Downstream);
        let composition = Composition::new();
        let leaf = match lines {
            Lines::Single => LayoutLeaf::register(layout_style)?,
            // Width is left to the parent: a line that does not wrap scrolls under the box rather than widening the layout, and one that wraps does so at whatever width the parent gives.
            Lines::Multi => {
                let (style, shape) = (Rc::clone(&style), Rc::clone(&shape));
                LayoutLeaf::measured(
                    layout_style.align_self_stretch(),
                    move |input: layout_core::MeasureInput| {
                        let style = style();
                        let width = match input.width {
                            AvailableSpace::Definite(width) => shape.wrap_width(width),
                            _ => None,
                        };
                        let shown = drawn_rows(&shape, value, caret, composition, &style, width);
                        (0.0, shown as f32 * crate::text_metrics::line_box(&style))
                    },
                )?
            }
        };
        let LayoutLeaf { node, rect } = leaf;
        node_cell.set(Some(node));
        let id = focus::next_id();
        let role = match lines {
            Lines::Single => focus::Role::TextInput,
            Lines::Multi => focus::Role::MultilineTextInput,
        };
        // As the kind that takes keys as text, so an app-level shortcut table stands aside while the caret is here.
        focus::register_with_role(id, focus::FocusKind::TextEntry, node, role);
        let remeasure = (lines == Lines::Multi)
            .then(|| remeasure(&shape, &style, node, rect, value, caret, composition));
        let follow_caret = {
            let (style, shape) = (Rc::clone(&style), Rc::clone(&shape));
            effect(move || {
                follow(
                    &shape,
                    &style(),
                    FollowState {
                        id,
                        node,
                        area: rect.get(),
                        value,
                        at: caret.get(),
                        affinity: affinity.get(),
                        composition,
                    },
                )
            })
        };
        let blink = Blink::new();
        let watching = blink.clone();
        let drawn_caret: Rc<Cell<Option<TextInput>>> = Rc::default();
        focus::provide_text_input(id, {
            let drawn_caret = Rc::clone(&drawn_caret);
            move || crate::text_input::placed(node, drawn_caret.get()?)
        });
        let mut input = InputHandle::new();
        input.answer(node, rect.read_only());
        Ok(Self {
            value,
            home: HomeSurface::here(),
            caret,
            affinity,
            anchor: signal(None),
            goal_x: None,
            style,
            shape,
            id,
            leaf: LayoutLeaf { node, rect },
            blink,
            composition,
            drawn_caret,
            clicks: Clicks::default(),
            drag: None,
            on_submit: None,
            on_cancel: None,
            on_key: None,
            underlines: None,
            _remeasure: remeasure,
            _blinking: effect(move || {
                let focused = focus::is_focused(id);
                watching.follow(focused);
                if !focused {
                    composition.settle(value, caret);
                }
            }),
            _follow_caret: follow_caret,
            _input: input,
        })
    }

    /// The muted hint shown while the value is empty, wrapped as the text would be.
    pub(crate) fn set_placeholder(&self, placeholder: String) {
        *self.shape.placeholder.borrow_mut() = placeholder;
        if self.shape.lines == Lines::Multi {
            self.shape.changed(self.leaf.node);
        }
    }

    /// Whether rows break at the box's width or only where a `\n` breaks them. An editor of one line never wraps.
    pub(crate) fn set_wrap(&self, wrap: TextWrap) {
        if self.shape.lines == Lines::Multi {
            self.shape.wrap.set(wrap);
            self.shape.changed(self.leaf.node);
        }
    }

    /// Draws `bullet` in place of every character. Rendering only: the value, the caret and every edit still work on the real text.
    pub(crate) fn set_mask(&self, bullet: char) {
        self.shape.mask.set(Some(bullet));
    }

    pub(crate) fn mask(&self) -> Option<char> {
        self.shape.mask.get()
    }

    /// The insertion point, to read and to move from outside.
    pub(crate) fn caret_handle(&self) -> Caret {
        Caret {
            at: self.caret,
            anchor: self.anchor,
        }
    }

    /// What is drawn for `text`: the text itself, or one mask character per character of it.
    pub(crate) fn shown(&self, text: &str) -> String {
        match self.mask() {
            Some(bullet) => text.chars().map(|_| bullet).collect(),
            None => text.to_string(),
        }
    }

    /// `text` in the rows the editor draws it in, at the width and scroll it was last laid out at.
    pub(crate) fn placed<'a>(&self, text: &'a str, style: &TextStyle) -> Placed<'a> {
        let area = self.leaf.rect.get();
        let frame = self.shape.frame(
            laid_width(self.leaf.node, area),
            area.height,
            style,
            self.shape.scroll_x.get(),
        );
        Placed::new(&self.shape.rows, text, style, self.mask(), frame)
    }

    /// The current caret byte offset, clamped to the text and snapped to a char boundary.
    pub(crate) fn caret_at(&self, text: &str) -> usize {
        floor_boundary(text, self.caret.get())
    }

    /// The selected byte range, low end first, or `None` when nothing is selected. An anchor sitting on the caret is not a selection — it is where one would start from.
    pub(crate) fn selection(&self, text: &str) -> Option<(usize, usize)> {
        let caret = self.caret_at(text);
        let anchor = floor_boundary(text, self.anchor.get()?);
        (anchor != caret).then(|| (anchor.min(caret), anchor.max(caret)))
    }

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

    /// Selects from `anchor` to `caret`, the caret on the row `affinity` says.
    fn select(&self, anchor: usize, caret: usize, affinity: Affinity) {
        self.anchor.set((anchor != caret).then_some(anchor));
        self.caret.set(caret);
        self.affinity.set(affinity);
    }

    /// What of `inserted` this editor keeps: the first line of it in one of a single line, every line in one of several, with the line breaks a platform writes made `\n` and no other control character.
    fn admitted(&self, inserted: &str) -> String {
        match self.shape.lines {
            Lines::Single => inserted
                .lines()
                .next()
                .unwrap_or_default()
                .chars()
                .filter(|c| !c.is_control())
                .collect(),
            Lines::Multi => inserted
                .replace("\r\n", "\n")
                .chars()
                .map(|c| if c == '\r' { '\n' } else { c })
                .filter(|c| matches!(c, '\n' | '\t') || !c.is_control())
                .collect(),
        }
    }

    /// Applies a key while focused, editing the bound signal and/or moving the caret. Returns whether the key was consumed.
    ///
    /// Home and End, and Command with the horizontal arrows on a Mac, go to the ends of the row on screen; the primary modifier with Home or End, and Command with the vertical arrows on a Mac, go to the ends of the text.
    pub(crate) fn edit(&mut self, key: &Key, mods: &ModifiersState) -> EventResult {
        // Before the key is even read: a caret that blinked out under the hand is missing at the one moment somebody is looking for it.
        self.blink.wake();
        let goal_x = self.goal_x.take();
        let multi = self.shape.lines == Lines::Multi;
        let mut text = self.value.get();
        let mut caret = self.caret_at(&text);
        let mut affinity = Affinity::Downstream;
        let mut edited = false;
        // Resolved after the match so each arm can still read the selection it is replacing.
        let anchor = mods.is_shift.then(|| {
            self.anchor
                .get()
                .map_or(caret, |a| floor_boundary(&text, a))
        });
        let command = keyboard_convention() == KeyboardConvention::Mac && mods.is_meta;
        let replace = |text: &mut String, caret: usize, with: &str| {
            let at = self.take_selection(text).unwrap_or(caret);
            text.insert_str(at, with);
            at + with.len()
        };
        match key {
            Key::Char('a') | Key::Char('A') if mods.is_primary() => {
                self.select(0, text.len(), Affinity::Downstream);
                return EventResult::Handled;
            }
            Key::Char('c') | Key::Char('C') if mods.is_primary() => {
                let Some(selected) = self.selected_text(&text) else {
                    return EventResult::Ignored;
                };
                services_core::set_clipboard_text(&selected);
                return EventResult::Handled;
            }
            Key::Char('x') | Key::Char('X') if mods.is_primary() => {
                let Some(selected) = self.selected_text(&text) else {
                    return EventResult::Ignored;
                };
                services_core::set_clipboard_text(&selected);
                caret = replace(&mut text, caret, "");
                edited = true;
            }
            Key::Char('v') | Key::Char('V') if mods.is_primary() => {
                let Some(pasted) = services_core::clipboard_text() else {
                    return EventResult::Ignored;
                };
                let pasted = self.admitted(&pasted);
                if pasted.is_empty() && self.selection(&text).is_none() {
                    return EventResult::Ignored;
                }
                caret = replace(&mut text, caret, &pasted);
                edited = true;
            }
            // Any other chord is a shortcut, not text.
            Key::Char(_) if mods.is_ctrl || mods.is_meta => return EventResult::Ignored,
            Key::Char(c) if !c.is_control() => {
                caret = replace(&mut text, caret, c.encode_utf8(&mut [0; 4]));
                edited = true;
            }
            Key::Named(NamedKey::Space) => {
                caret = replace(&mut text, caret, " ");
                edited = true;
            }
            Key::Named(NamedKey::Enter) if multi => {
                caret = replace(&mut text, caret, "\n");
                edited = true;
            }
            Key::Named(NamedKey::Enter) => {
                if let Some(submit) = &self.on_submit {
                    submit();
                }
                return EventResult::Handled;
            }
            Key::Named(NamedKey::Tab) if multi => {
                caret = replace(&mut text, caret, TAB_INSERT);
                edited = true;
            }
            Key::Named(NamedKey::Tab) => {
                match mods.is_shift {
                    true => focus::focus_prev(),
                    false => focus::focus_next(),
                }
                return EventResult::Handled;
            }
            Key::Named(NamedKey::Backspace) => {
                if self.selection(&text).is_none() {
                    if caret == 0 {
                        return EventResult::Ignored;
                    }
                    let prev = prev_boundary(&text, caret);
                    text.replace_range(prev..caret, "");
                    caret = prev;
                } else {
                    caret = replace(&mut text, caret, "");
                }
                edited = true;
            }
            Key::Named(NamedKey::Delete) => {
                if self.selection(&text).is_none() {
                    if caret >= text.len() {
                        return EventResult::Ignored;
                    }
                    let next = next_boundary(&text, caret);
                    text.replace_range(caret..next, "");
                } else {
                    caret = replace(&mut text, caret, "");
                }
                edited = true;
            }
            Key::Named(NamedKey::Home) if mods.is_primary() => caret = 0,
            Key::Named(NamedKey::End) if mods.is_primary() => caret = text.len(),
            Key::Named(NamedKey::ArrowUp) if command => caret = 0,
            Key::Named(NamedKey::ArrowDown) if command => caret = text.len(),
            Key::Named(NamedKey::ArrowLeft) if command => caret = self.row_start(&text, caret),
            Key::Named(NamedKey::Home) => caret = self.row_start(&text, caret),
            Key::Named(NamedKey::ArrowRight) if command => {
                (caret, affinity) = self.row_end(&text, caret);
            }
            Key::Named(NamedKey::End) => (caret, affinity) = self.row_end(&text, caret),
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
            Key::Named(NamedKey::ArrowUp) if multi => {
                (caret, affinity) = self.step_row(&text, caret, goal_x, -1);
            }
            Key::Named(NamedKey::ArrowDown) if multi => {
                (caret, affinity) = self.step_row(&text, caret, goal_x, 1);
            }
            Key::Named(NamedKey::Escape) => {
                // Before the keyboard goes back, so a caller watching focus reads what follows as the consequence.
                if let Some(cancel) = &self.on_cancel {
                    cancel();
                }
                focus::release(self.id);
                return EventResult::Handled;
            }
            _ => return EventResult::Ignored,
        }
        if edited {
            self.value.set(text);
        }
        self.caret.set(caret);
        self.affinity.set(affinity);
        // An anchor that caught up with the caret is no selection, and keeping it would make the next unshifted arrow collapse to a range of nothing. An edit drops it outright: kept, the anchor sat where the caret was before the letter went in, so the next keystroke replaced what had just been typed.
        self.anchor
            .set(anchor.filter(|anchor| *anchor != caret && !edited));
        EventResult::Handled
    }

    fn row_start(&self, text: &str, caret: usize) -> usize {
        let placed = self.placed(text, &(self.style)());
        placed
            .lines
            .row(placed.row_of(caret, self.affinity.get()))
            .start
    }

    /// The end of the caret's row, and the affinity that keeps the caret on it rather than at the start of the row it wraps into.
    fn row_end(&self, text: &str, caret: usize) -> (usize, Affinity) {
        let placed = self.placed(text, &(self.style)());
        let row = placed.row_of(caret, self.affinity.get());
        (placed.lines.row(row).end, end_affinity(&placed, row))
    }

    /// The caret one row up (`-1`) or down (`1`) at the x the run of vertical moves began at, or the start or end of the text past the first or last row.
    fn step_row(
        &mut self,
        text: &str,
        caret: usize,
        goal_x: Option<f32>,
        by: isize,
    ) -> (usize, Affinity) {
        let placed = self.placed(text, &(self.style)());
        let affinity = self.affinity.get();
        let row = placed.row_of(caret, affinity);
        let x = goal_x.unwrap_or_else(|| placed.x_at(row, caret));
        self.goal_x = Some(x);
        match row.checked_add_signed(by) {
            Some(next) if next < placed.lines.len() => {
                let hit = placed.hit_row(next, x);
                (hit.at, hit.affinity)
            }
            Some(_) => (text.len(), Affinity::Downstream),
            None => (0, Affinity::Downstream),
        }
    }

    /// Follows the input method composing into the editor: the composition is drawn until it is committed, and a selection gives way to it as soon as it starts.
    fn compose(&mut self, ime: &Ime) -> EventResult {
        match self.composition.apply(ime) {
            Step::Ignored => return EventResult::Ignored,
            Step::Began => {
                let mut text = self.value.get();
                if let Some(at) = self.take_selection(&mut text) {
                    self.value.set(text);
                    self.caret.set(at);
                }
                self.anchor.set(None);
            }
            Step::Changed | Step::Ended => {}
            Step::Committed(committed) => self.insert(&committed),
        }
        self.goal_x = None;
        self.affinity.set(Affinity::Downstream);
        self.blink.wake();
        EventResult::Handled
    }

    /// Types `inserted` at the caret, in place of the selection, as much of it as [`admitted`](Self::admitted) keeps.
    fn insert(&self, inserted: &str) {
        let inserted = self.admitted(inserted);
        if inserted.is_empty() {
            return;
        }
        let mut text = self.value.get();
        let caret = self
            .take_selection(&mut text)
            .unwrap_or_else(|| self.caret_at(&text));
        text.insert_str(caret, &inserted);
        self.value.set(text);
        self.caret.set(caret + inserted.len());
        self.anchor.set(None);
    }

    /// The run a press selects at `hit`, by `granularity`.
    fn run_at(&self, text: &str, hit: Hit, granularity: Granularity) -> (usize, usize) {
        let range = match granularity {
            Granularity::Char => hit.at..hit.at,
            Granularity::Word => word_at(text, hit.under),
            Granularity::Line => match self.shape.lines {
                Lines::Single => 0..text.len(),
                Lines::Multi => line_at(text, hit.under),
            },
        };
        (range.start, range.end)
    }

    /// A primary press at `(x, y)` inside the box: positions the caret, takes a word on a double-click and a line on a triple-click, extends the selection with Shift held, and starts a drag that goes on extending it.
    fn press(&mut self, x: f32, y: f32, source: &PointerSource) {
        focus::request_from_pointer(self.id);
        self.goal_x = None;
        self.blink.wake();
        let count = self.clicks.press(x, y);
        let granularity = Granularity::of_clicks(count);
        // Read the text out before setting the caret: a `set` inside a `with` closure would re-borrow the runtime.
        let text = self.value.get();
        let style = (self.style)();
        let area = self.leaf.rect.get();
        let placed = self.placed(&text, &style);
        let hit = placed.hit(x - area.x, y - area.y);
        let extending = granularity == Granularity::Char && crate::keyboard::modifiers().is_shift;
        let origin = match extending {
            true => {
                let from = self
                    .anchor
                    .get()
                    .map_or_else(|| self.caret_at(&text), |a| floor_boundary(&text, a));
                (from, from)
            }
            false => self.run_at(&text, hit, granularity),
        };
        let (anchor, caret, affinity) = match extending {
            true => (origin.0, hit.at, hit.affinity),
            false if granularity == Granularity::Char => (hit.at, hit.at, hit.affinity),
            false => (origin.0, origin.1, run_end_affinity(&placed, origin.1)),
        };
        self.select(anchor, caret, affinity);
        // A finger drags the page it is on, not a selection.
        self.drag = matches!(source, PointerSource::Mouse).then_some(Drag {
            granularity,
            origin,
        });
    }

    /// The pointer moved to `(x, y)` with a press held: the selection runs from what the press took to where the pointer is, by the press's granularity.
    fn drag_to(&mut self, drag: Drag, x: f32, y: f32) {
        let text = self.value.get();
        let style = (self.style)();
        let area = self.leaf.rect.get();
        let placed = self.placed(&text, &style);
        let hit = placed.hit(x - area.x, y - area.y);
        let (low, high) = self.run_at(&text, hit, drag.granularity);
        let (start, end) = drag.origin;
        let (anchor, caret) = match low < start {
            true => (end, low),
            false => (start, high.max(end)),
        };
        let affinity = match drag.granularity {
            Granularity::Char => hit.affinity,
            _ => run_end_affinity(&placed, caret),
        };
        self.select(anchor, caret, affinity);
    }

    /// A horizontal wheel or swipe over a box whose text runs past it scrolls the text sideways. One that goes more down than across is the scroll around the box's to take.
    fn scroll_by(&self, delta: &platform_core::ScrollDelta) -> EventResult {
        let (dx, dy) = delta.pixels();
        if dx == 0.0 || dx.abs() < dy.abs() {
            return EventResult::Ignored;
        }
        let text = self.value.get();
        let style = (self.style)();
        let max = self.placed(&text, &style).max_scroll_x();
        let scroll = self.shape.scroll_x.peek();
        let next = (scroll - dx).clamp(0.0, max);
        if next == scroll {
            return EventResult::Ignored;
        }
        self.shape.scroll_x.set(next);
        EventResult::Handled
    }

    pub(crate) fn on_event(&mut self, event: &Event) -> EventResult {
        let rect = self.leaf.rect.get();
        match event {
            Event::PointerPressed {
                x,
                y,
                button: PointerButton::Primary,
                source,
            } => {
                let (x, y) = (*x as f32, *y as f32);
                if !rect.contains(x, y) {
                    return EventResult::Ignored;
                }
                self.press(x, y, source);
                EventResult::Handled
            }
            Event::PointerMoved { x, y, .. } => match self.drag {
                Some(drag) => {
                    self.drag_to(drag, *x as f32, *y as f32);
                    EventResult::Handled
                }
                None => EventResult::Ignored,
            },
            Event::PointerReleased { .. } => {
                self.drag = None;
                EventResult::Ignored
            }
            Event::Scrolled { delta, x, y } if rect.contains(*x as f32, *y as f32) => {
                self.scroll_by(delta)
            }
            Event::Ime(ime) if focus::is_key_target(self.id) => self.compose(ime),
            // The input method owns the keys while it composes, whichever reach the editor anyway.
            Event::KeyPressed { .. }
                if focus::is_key_target(self.id) && self.composition.is_active() =>
            {
                EventResult::Handled
            }
            Event::KeyPressed { key, modifiers, .. } if focus::is_key_target(self.id) => {
                if self.on_key.as_ref().is_some_and(|f| f(key, *modifiers)) {
                    return EventResult::Handled;
                }
                self.edit(key, modifiers)
            }
            _ => EventResult::Ignored,
        }
    }

    /// Whether the keyboard is in the editor, and what a reader is told it is.
    pub(crate) fn semantics(&self) -> Semantics {
        let role = match self.shape.lines {
            Lines::Single => renderer_core::Role::TextInput,
            Lines::Multi => renderer_core::Role::MultilineTextInput,
        };
        let mut semantics = Semantics::of(role);
        focus::describe(self.id, &mut semantics, None);
        semantics
    }

    pub(crate) fn view(&self) -> RenderNode {
        let area = self.leaf.rect.get();
        let text = self.value.get();
        let style = (self.style)();
        let paint = style.color;
        let focused = focus::is_focused(self.id);
        let caret = self.caret_at(&text);
        let composed = focused
            .then(|| self.composition.get())
            .flatten()
            .map(|preedit| composition::compose(&text, caret, &preedit));
        let drawn = composed.as_ref().map_or(text.as_str(), |c| c.text.as_str());
        let placed = self.placed(drawn, &style);
        let line_h = placed.frame.line_h;
        let pinned = pinned(style.clone());
        let height = match self.shape.lines {
            Lines::Single => area.height - placed.frame.top * 2.0,
            Lines::Multi => area.height.max(line_h),
        };
        let placeholder = self.shape.placeholder.borrow();
        // The editor itself stays live: the caret and hit-test still work, so it is typable from empty.
        let text_node = if drawn.is_empty() && !placeholder.is_empty() {
            let faded = pinned.clone().with_color(paint.faded(PLACEHOLDER_OPACITY));
            let width = placed.frame.wrap.unwrap_or(area.width);
            let rect = Rect::new(0.0, placed.frame.top, width, height);
            RenderNode::text(placeholder.clone(), rect, faded)
        } else {
            let rect = Rect::new(
                -placed.frame.scroll_x,
                placed.frame.top,
                placed.text_width(),
                height,
            );
            RenderNode::text(self.shown(drawn), rect, pinned)
        };
        let shifted = |at: usize| composed.as_ref().map_or(at, |c| c.shift(at));
        let spans = self
            .underlines
            .as_ref()
            .map_or_else(Vec::new, |spans| spans());
        let underlines: Vec<RenderNode> = spans
            .into_iter()
            .flat_map(|underline| {
                let from = floor_boundary(&text, underline.range.start);
                let to = floor_boundary(&text, underline.range.end.max(from));
                let fill = RectStyle::default().with_fill(underline.color);
                underline_marks(&placed, shifted(from)..shifted(to))
                    .into_iter()
                    .map(move |rect| RenderNode::rect(rect, fill))
            })
            .collect();
        if !focused {
            self.drawn_caret.set(None);
            let layers = std::iter::once(text_node).chain(underlines);
            return self
                .leaf
                .at_layout_position_as(|| self.semantics(), clipped(area, layers));
        }
        let highlight = match composed {
            Some(_) => None,
            None => self.selection(&text),
        }
        .map(|(from, to)| {
            let fill = RectStyle::default().with_fill(paint.faded(0.25));
            let bands = placed
                .bands(from..to, true)
                .into_iter()
                .map(|(row, x0, x1)| {
                    let band = Rect::new(x0, placed.row_top(row), (x1 - x0).max(1.0), line_h);
                    RenderNode::rect(band, fill)
                });
            RenderNode::group(bands)
        });
        let composing = composed
            .as_ref()
            .map(|c| composing_marks(&placed, c, paint));
        let (caret_at, affinity) = match &composed {
            Some(c) => (c.anchor, Affinity::Downstream),
            None => (caret, self.affinity.get()),
        };
        let caret_rect = placed.caret_rect(caret_at, affinity);
        self.drawn_caret.set(Some(TextInput {
            field: self.id,
            caret: caret_rect,
            secret: self.mask().is_some(),
        }));
        // Read here and nowhere else, so the caret is the only thing on the surface redrawing on the blink's account. An input method that asked for no caret, or is converting a part, gets none.
        let caret_node = composed
            .as_ref()
            .is_none_or(|c| c.caret.is_some())
            .then(|| {
                let lit = paint.faded(self.blink.opacity());
                RenderNode::rect(caret_rect, RectStyle::default().with_fill(lit))
            });
        let layers = highlight
            .into_iter()
            .chain(std::iter::once(text_node))
            .chain(underlines)
            .chain(composing)
            .chain(caret_node);
        self.leaf.at_layout_position_editing(
            || self.semantics(),
            Some(caret_rect),
            clipped(area, layers),
        )
    }
}

impl Drop for Editor {
    fn drop(&mut self) {
        self.home.with_entered(|| {
            focus::unregister(self.id);
        });
    }
}

/// `layers` cut to the box: a line longer than it scrolls under its edge rather than drawing past it.
fn clipped(area: Rect, layers: impl IntoIterator<Item = RenderNode>) -> RenderNode {
    RenderNode::clip(
        Rect::new(0.0, 0.0, area.width, area.height),
        BorderRadius::zero(),
        layers,
    )
}

/// `style` with its line height said outright, as the rows the caret, the selection and the underlines are placed on: a line box left to the face grows wherever a character falls back to another face — a kana in a Latin font — and every row after it drifts from the rows the editor placed its marks on.
fn pinned(mut style: TextStyle) -> TextStyle {
    if style.font_size > 0.0 {
        style.line_height =
            LineHeight::Times(crate::text_metrics::line_box(&style) / style.font_size);
    }
    style
}

/// The affinity a caret at the end of `row` keeps to stay on it rather than at the start of the row it wraps into.
fn end_affinity(placed: &Placed<'_>, row: usize) -> Affinity {
    match placed.lines.wraps_after(row) {
        true => Affinity::Upstream,
        false => Affinity::Downstream,
    }
}

/// The affinity a caret at the end of a run a press selected keeps, so it stands after the run's last letter rather than at the start of the next row.
fn run_end_affinity(placed: &Placed<'_>, at: usize) -> Affinity {
    let row = placed.row_of(at, Affinity::Upstream);
    match placed.lines.row(row).end == at {
        true => end_affinity(placed, row),
        false => Affinity::Downstream,
    }
}

/// The rects of the line under `run`: one stretch per row it covers, along the bottom of the line, or a short mark where an empty one starts.
fn underline_marks(placed: &Placed<'_>, run: Range<usize>) -> Vec<Rect> {
    let line_h = placed.frame.line_h;
    let mark = |row: usize, x: f32, width: f32| {
        Rect::new(
            x,
            placed.row_top(row) + line_h - UNDERLINE_THICKNESS,
            width,
            UNDERLINE_THICKNESS,
        )
    };
    if run.is_empty() {
        let row = placed.row_of(run.start, Affinity::Downstream);
        return vec![mark(row, placed.x_at(row, run.start), POINT_MARK)];
    }
    placed
        .bands(run, false)
        .into_iter()
        .map(|(row, x0, x1)| mark(row, x0, (x1 - x0).max(1.0)))
        .collect()
}

/// The lines under a composition, one stretch per row it is drawn across.
fn composing_marks(
    placed: &Placed<'_>,
    composed: &Composed,
    color: renderer_core::Paint,
) -> RenderNode {
    let mut marks = Vec::new();
    for (row, x0, x1) in placed.bands(composed.span.clone(), false) {
        let range = placed.lines.row(row);
        let converting = composed.converting.as_ref().and_then(|part| {
            let (from, to) = (part.start.max(range.start), part.end.min(range.end));
            (from <= to).then(|| (placed.x_at(row, from), placed.x_at(row, to)))
        });
        let bottom = placed.row_top(row) + placed.frame.line_h;
        marks.extend(composition::underlines(x0, x1, converting, bottom, color));
    }
    RenderNode::group(marks)
}

/// How far down a box taller than its single line puts that line, so the text sits mid-box and the whole box is the control.
fn line_inset(height: f32, font_size: f32) -> f32 {
    ((height - crate::text_metrics::single_line_box(font_size)) / 2.0)
        .max(0.0)
        .round()
}

/// The width the box's rows wrap at: as wide as layout made it before rounding took any off, since that is the width it was measured at and a row measured to fit must not break again when drawn.
pub(crate) fn laid_width(node: NodeId, rect: Rect) -> f32 {
    if rect.width <= 0.0 {
        return rect.width;
    }
    layout_reactive::unrounded_width(node).map_or(rect.width, |unrounded| rect.width.max(unrounded))
}

/// How many rows tall the box draws its text, with the composition in progress drawn into it: one that runs onto a new row grows the box as typing would.
fn drawn_rows(
    shape: &Shape,
    value: RwSignal<String>,
    caret: RwSignal<usize>,
    composition: Composition,
    style: &TextStyle,
    width: Option<f32>,
) -> usize {
    match composition.get() {
        Some(preedit) => {
            let at = caret.get();
            let composed = value.with(|text| composition::compose(text, at, &preedit));
            shape.shown_rows(&composed.text, style, width)
        }
        None => value.with(|text| shape.shown_rows(text, style, width)),
    }
}

/// Dirties the leaf only when the rows or the style move its height, so a new row, a programmatic load and a restyle reach the layout while a keystroke within a row costs none. The width is read without subscribing: a new width is a layout already measuring again.
fn remeasure(
    shape: &Rc<Shape>,
    style: &Rc<dyn Fn() -> TextStyle>,
    node: NodeId,
    rect: RwSignal<Rect>,
    value: RwSignal<String>,
    caret: RwSignal<usize>,
    composition: Composition,
) -> Effect {
    let (shape, style) = (Rc::clone(shape), Rc::clone(style));
    effect(move || {
        let style = style();
        let width = shape.wrap_width(laid_width(node, rect.peek()));
        let shown = drawn_rows(&shape, value, caret, composition, &style, width);
        let unchanged = shape
            .measured
            .borrow()
            .as_ref()
            .is_some_and(|(count, at, measured)| {
                *count == shown && *at == width && measured.same_extent(&style)
            });
        if unchanged {
            return;
        }
        *shape.measured.borrow_mut() = Some((shown, width, style));
        mark_dirty(node).ok();
    })
}

/// What the effect that keeps the caret in view reads.
struct FollowState {
    id: FocusId,
    node: NodeId,
    area: Rect,
    value: RwSignal<String>,
    at: usize,
    affinity: Affinity,
    composition: Composition,
}

/// Keeps the caret in view: sideways by scrolling the text under a box it runs past, and in an editor of several lines by bringing its row into view of the scroll the box sits in, which a reveal of the whole box could not do once the box is taller than the view.
///
/// Back at the start once the keyboard leaves, so a field nobody is typing into shows how its value begins.
fn follow(shape: &Shape, style: &TextStyle, state: FollowState) {
    let FollowState {
        id,
        node,
        area,
        value,
        at,
        affinity,
        composition,
    } = state;
    if !focus::is_focused(id) {
        if shape.scroll_x.peek() != 0.0 {
            shape.scroll_x.set(0.0);
        }
        return;
    }
    let preedit = composition.get();
    let width = laid_width(node, area);
    // Copied out rather than read in place: measuring asks for the writing direction, which may not be read while the value is borrowed.
    let text = value.get();
    let at = floor_boundary(&text, at);
    let composed = preedit.as_ref().map(|p| composition::compose(&text, at, p));
    let (drawn, at, affinity) = match &composed {
        Some(c) => (c.text.as_str(), c.anchor, Affinity::Downstream),
        None => (text.as_str(), at, affinity),
    };
    let frame = shape.frame(width, area.height, style, 0.0);
    let placed = Placed::new(&shape.rows, drawn, style, shape.mask.get(), frame);
    let (caret, max) = (placed.caret_rect(at, affinity), placed.max_scroll_x());
    let scroll = shape.scroll_x.peek();
    let kept = keep_in_view(scroll, caret.x, width, max);
    if kept != scroll {
        shape.scroll_x.set(kept);
    }
    if shape.lines == Lines::Single {
        return;
    }
    let Some(viewport) = crate::scroll_viewports::enclosing_scroll_viewport(node) else {
        return;
    };
    viewport.reveal_rect(
        Rect::new(
            area.x + caret.x - kept,
            area.y + caret.y,
            caret.width,
            caret.height,
        ),
        0.0,
    );
}

/// The scroll that shows a caret at `x` in a box `width` wide, moving `scroll` as little as it takes and never past `max`.
fn keep_in_view(scroll: f32, x: f32, width: f32, max: f32) -> f32 {
    let mut kept = scroll.min(max);
    if x < kept {
        kept = x;
    } else if x + CARET_WIDTH > kept + width {
        kept = x + CARET_WIDTH - width;
    }
    kept.clamp(0.0, max.max(0.0))
}

#[cfg(test)]
#[path = "text_editor_test.rs"]
mod tests;
