//! [`TextArea`]: the multi-line editor — caret, selection, and a height measured from the rows it is drawn in.

use std::cell::{Cell, RefCell};
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use geometry_core::Rect;
use layout_core::{AvailableSpace, LayoutError, LayoutStyle, NodeId};
use platform_core::{
    Event, Ime, Key, KeyboardConvention, ModifiersState, NamedKey, PointerButton, TextInput,
    keyboard_convention,
};
use reactive_core::{Effect, RwSignal, effect, signal};
use renderer_core::{RectStyle, ShapeStyle, TextStyle, TextWrap};
use ui_tree::{Component, EventResult, RenderNode};

use crate::caret::Blink;
use crate::composition::{self, Composed, Composition, Step};
use crate::context::{mark_dirty, new_measured_leaf};
use crate::focus::{self, FocusId};
use crate::impl_leaf_widget;
use crate::input_region::InputHandle;
use crate::layout_leaf::LayoutLeaf;
use crate::surface_context::HomeSurface;
use crate::visual_lines::{Affinity, VisualLines};

/// Width of the caret, in logical px.
const CARET_WIDTH: f32 = 1.5;
/// Where an area that does not wrap draws its text: wide enough that a line never meets the edge, so it overflows and an ancestor clips it.
const NO_WRAP_WIDTH: f32 = 1.0e6;
/// Inserted by the Tab key (soft tabs).
const TAB_INSERT: &str = "    ";

/// A multi-line editable text area bound to a `RwSignal<String>` — the multi-line sibling of [`Input`](crate::Input). A base primitive: unstyled (wrap it in a `box` for a border/background) and keyboard-driven. It requests focus on tap, positions the caret at the click, edits the bound signal from key events (typing, Enter for a newline, Backspace/Delete joining lines, arrows in all four directions, Home/End, Tab), and draws a caret.
///
/// By default only `\n` breaks a line and a long one overflows sideways, which is what a code editor and a line-number gutter beside it expect. [`text_wrap`](Self::text_wrap) with [`TextWrap::Wrap`] breaks lines at the area's width instead, between words and inside a word too long for a row, as a paragraph and an HTML `<textarea>` do; the caret then moves by the rows on screen. Its measured height grows with the rows it is drawn in, so wrapping it in a [`LayoutScrollArea`](crate::LayoutScrollArea) gives a scrolling editor. Selection (`Shift`+arrows, `Mod+A`, shift-click) with copy, cut and paste on the primary modifier, Command on a Mac and Ctrl elsewhere, newlines and all. An input method composes in place, as in [`Input`](crate::Input): its composition is drawn at the caret, underlined and wrapped with the text, and what it commits is typed there.
pub struct TextArea {
    value: RwSignal<String>,
    home: HomeSurface,
    // Reactive so a bare caret move re-renders even when the text is unchanged.
    caret: RwSignal<usize>,
    affinity: RwSignal<Affinity>,
    // Either side of the caret: a selection extended leftwards has its anchor after it.
    anchor: RwSignal<Option<usize>>,
    // Kept across consecutive Up/Down presses, so passing through a short row does not pull the caret to its end for good.
    goal_x: Option<f32>,
    style: Rc<dyn Fn() -> TextStyle>,
    rows: Rc<Rows>,
    id: FocusId,
    leaf: LayoutLeaf,
    _remeasure: Effect,
    blink: Blink,
    composition: Composition,
    // Where the caret was last drawn while the area held the keyboard, in the leaf's own coordinates.
    drawn_caret: Rc<Cell<Option<TextInput>>>,
    _blinking: Effect,
    // A reveal of the whole area cannot keep the caret in view once the area is taller than the view.
    _follow_caret: Effect,
    _input: InputHandle,
}

/// What the measure, the effects and the view share about the rows the area is drawn in.
struct Rows {
    wrap: Cell<TextWrap>,
    placeholder: RefCell<String>,
    cached: RefCell<Option<Laid>>,
    // What the area was last measured from: its row count, the width that count was taken at, and the style.
    measured: RefCell<Option<(usize, Option<f32>, TextStyle)>>,
}

/// The rows of one text at one width, kept until the text, the width, the style or the fonts move.
struct Laid {
    text: String,
    width: f32,
    style: TextStyle,
    generation: u64,
    lines: Rc<VisualLines>,
}

impl Rows {
    fn wrap_width(&self, width: f32) -> Option<f32> {
        (self.wrap.get() == TextWrap::Wrap && width > 0.0).then_some(width)
    }

    fn lines(&self, text: &str, style: &TextStyle, width: Option<f32>) -> Rc<VisualLines> {
        let Some(width) = width else {
            return Rc::new(VisualLines::of(text, style, None));
        };
        let generation = renderer_core::text_metrics_generation();
        if let Some(laid) = self.cached.borrow().as_ref()
            && laid.text == text
            && laid.width == width
            && laid.generation == generation
            && laid.style.same_extent(style)
        {
            return Rc::clone(&laid.lines);
        }
        let lines = Rc::new(VisualLines::of(text, style, Some(width)));
        *self.cached.borrow_mut() = Some(Laid {
            text: text.to_string(),
            width,
            style: style.clone(),
            generation,
            lines: Rc::clone(&lines),
        });
        lines
    }

    /// How many rows tall what is drawn is: the text, or the placeholder standing in for it.
    fn shown_rows(&self, text: &str, style: &TextStyle, width: Option<f32>) -> usize {
        if text.is_empty() {
            let placeholder = self.placeholder.borrow();
            VisualLines::of(&placeholder, style, width).len()
        } else {
            self.lines(text, style, width).len()
        }
    }

    /// Forgets the last measurement, so the next change to the text measures again whatever it holds.
    fn changed(&self, node: NodeId) {
        *self.measured.borrow_mut() = None;
        mark_dirty(node).ok();
    }
}

/// One text laid out in rows, with the style its prefixes are measured in.
struct Placed<'a> {
    text: &'a str,
    lines: Rc<VisualLines>,
    flat: TextStyle,
    // The width rows wrap at, `None` when they do not.
    width: Option<f32>,
}

impl Placed<'_> {
    fn row_of(&self, at: usize, affinity: Affinity) -> usize {
        self.lines.row_of(at, affinity)
    }

    /// Pixel x of byte `at` on `row`: the advance of the row up to it, held inside the box a wrapped row is drawn in, which the space a row broke on hangs past.
    fn x_at(&self, row: usize, at: usize) -> f32 {
        let start = self.lines.row(row).start.min(at);
        let x = crate::text_metrics::measure_text(
            &self.text[start..at.min(self.text.len())],
            None,
            NO_WRAP_WIDTH,
            &self.flat,
        )
        .0;
        match self.width {
            Some(width) => x.min((width - CARET_WIDTH).max(0.0)),
            None => x,
        }
    }

    fn caret_x(&self, at: usize, affinity: Affinity) -> f32 {
        self.x_at(self.row_of(at, affinity), at)
    }

    /// The offset on `row` whose caret is nearest `x`, and the affinity that keeps it there: the end of a wrapped row is also the start of the next.
    fn offset_at(&self, row: usize, x: f32) -> (usize, Affinity) {
        let range = self.lines.row(row);
        let line = &self.text[range.clone()];
        let mut best = range.start;
        let mut best_dx = f32::MAX;
        let mut idx = 0;
        loop {
            let dx = (self.x_at(row, range.start + idx) - x).abs();
            if dx < best_dx {
                best_dx = dx;
                best = range.start + idx;
            }
            if idx >= line.len() {
                break;
            }
            idx = next_boundary(line, idx);
        }
        let affinity = if best == range.end && self.lines.wraps_after(row) {
            Affinity::Upstream
        } else {
            Affinity::Downstream
        };
        (best, affinity)
    }
}

impl TextArea {
    /// A multi-line editor, and whether the keyboard is in it. See [`Input::semantics`](crate::Input).
    fn semantics(&self) -> renderer_core::Semantics {
        let role = renderer_core::Role::MultilineTextInput;
        renderer_core::Semantics::of(role)
            .in_state(focus::is_focused(self.id), None, false)
            .focusable(focus::focusable_of(self.id, role, None))
    }

    /// An area in exactly `style_fn`'s style: the opt-out from the tree above, the surface's family included. [`declaring`](Self::declaring) is the one that follows it.
    pub fn new(
        value: RwSignal<String>,
        layout_style: LayoutStyle,
        style_fn: impl Fn() -> TextStyle + 'static,
    ) -> Result<Self, LayoutError> {
        Self::build(value, layout_style, |_| Rc::new(style_fn))
    }

    /// An area styled by what the tree above it declared, amended by whatever it says for itself — the counterpart of [`Text::declaring`](crate::Text::declaring) and [`Input::declaring`](crate::Input::declaring), and what keeps an editor in the surface's family ([`set_font_family`](crate::set_font_family)) and at the size of the text around it.
    pub fn declaring(
        value: RwSignal<String>,
        layout_style: LayoutStyle,
        style_fn: impl Fn(TextStyle) -> TextStyle + 'static,
    ) -> Result<Self, LayoutError> {
        Self::build(value, layout_style, |node| {
            Rc::new(move || style_fn(crate::inherit::inherited_text_style_at(node.get())))
        })
    }

    fn build(
        value: RwSignal<String>,
        layout_style: LayoutStyle,
        style: impl FnOnce(Rc<Cell<Option<NodeId>>>) -> Rc<dyn Fn() -> TextStyle>,
    ) -> Result<Self, LayoutError> {
        // The measure closure reads the style and exists before the node does, so an inheriting style finds its node through this cell.
        let node_cell = Rc::new(Cell::new(None));
        let declared = style(Rc::clone(&node_cell));
        let rows = Rc::new(Rows {
            wrap: Cell::new(TextWrap::NoWrap),
            placeholder: RefCell::new(String::new()),
            cached: RefCell::new(None),
            measured: RefCell::new(None),
        });
        // A case that changed a letter's length would put the caret and the selection, which index the value, between the wrong letters.
        let style: Rc<dyn Fn() -> TextStyle> = {
            let rows = Rc::clone(&rows);
            Rc::new(move || {
                declared()
                    .with_text_case(renderer_core::TextCase::AsWritten)
                    .with_text_wrap(rows.wrap.get())
            })
        };
        let caret = signal(value.with(|s| s.len()));
        let composition = Composition::new();
        // Width is left to the parent: a line that does not wrap overflows rather than widening the layout, and one that wraps does so at whatever width the parent gives.
        let measure = {
            let style = Rc::clone(&style);
            let rows = Rc::clone(&rows);
            Box::new(move |input: layout_core::MeasureInput| {
                let style = style();
                let width = match input.width {
                    AvailableSpace::Definite(width) => rows.wrap_width(width),
                    _ => None,
                };
                let shown = drawn_rows(&rows, value, caret, composition, &style, width);
                (0.0, shown as f32 * crate::text_metrics::line_box(&style))
            })
        };
        let (node, rect) = new_measured_leaf(layout_style.align_self_stretch(), measure)?;
        node_cell.set(Some(node));
        let affinity = signal(Affinity::Downstream);
        let id = focus::next_id();
        focus::register_with_role(
            id,
            focus::FocusKind::TextEntry,
            node,
            focus::Role::MultilineTextInput,
        );
        // Dirties the leaf only when the rows or the style move its height, so a new row, a programmatic load and a restyle reach the layout while a keystroke within a row costs none. The width is read without subscribing: a new width is a layout already measuring again.
        let remeasure = {
            let style = Rc::clone(&style);
            let rows = Rc::clone(&rows);
            effect(move || {
                let style = style();
                let width = rows.wrap_width(laid_width(node, rect.peek()));
                let shown = drawn_rows(&rows, value, caret, composition, &style, width);
                let unchanged =
                    rows.measured
                        .borrow()
                        .as_ref()
                        .is_some_and(|(count, at, measured)| {
                            *count == shown && *at == width && measured.same_extent(&style)
                        });
                if unchanged {
                    return;
                }
                *rows.measured.borrow_mut() = Some((shown, width, style));
                mark_dirty(node).ok();
            })
        };
        let follow_caret = {
            let style = Rc::clone(&style);
            let rows = Rc::clone(&rows);
            effect(move || {
                if !focus::is_focused(id) {
                    return;
                }
                let Some(viewport) = crate::scroll_viewports::enclosing_scroll_viewport(node)
                else {
                    return;
                };
                let area = rect.get();
                let style = style();
                let line_h = crate::text_metrics::line_box(&style);
                let (at, affinity) = (caret.get(), affinity.get());
                let preedit = composition.get();
                let (x, row) = value.with(|text| {
                    let at = char_boundary_at_or_before(text, at);
                    let composed = preedit.as_ref().map(|p| composition::compose(text, at, p));
                    let (drawn, at, affinity) = match &composed {
                        Some(c) => (c.text.as_str(), c.anchor, Affinity::Downstream),
                        None => (text.as_str(), at, affinity),
                    };
                    let placed = place(&rows, drawn, &style, laid_width(node, area));
                    (placed.caret_x(at, affinity), placed.row_of(at, affinity))
                });
                viewport.reveal_rect(
                    Rect {
                        x: area.x + x,
                        y: area.y + row as f32 * line_h,
                        width: CARET_WIDTH,
                        height: line_h,
                    },
                    0.0,
                );
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
            rows,
            id,
            leaf: LayoutLeaf { node, rect },
            _remeasure: remeasure,
            blink,
            composition,
            drawn_caret,
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

    /// A muted hint shown while the value is empty, wrapped as the text would be.
    pub fn placeholder(self, p: impl Into<String>) -> Self {
        *self.rows.placeholder.borrow_mut() = p.into();
        self.rows.changed(self.leaf.node);
        self
    }

    /// Whether lines break at the area's width ([`TextWrap::Wrap`]) or only where a `\n` breaks them ([`TextWrap::NoWrap`], the default), whatever the style around the area says.
    pub fn text_wrap(self, wrap: TextWrap) -> Self {
        self.rows.wrap.set(wrap);
        self.rows.changed(self.leaf.node);
        self
    }

    /// Gives keyboard focus to this area (as a tap would), leaving the caret where it was. For programmatic focus — e.g. a container autofocusing the editor when its tab or window becomes active.
    pub fn request_focus(&self) {
        focus::request(self.id);
    }

    /// Whether this area currently holds keyboard focus.
    pub fn focused(&self) -> bool {
        focus::is_focused(self.id)
    }

    /// A `Copy` [`focus::FocusHandle`] to this area, so a caller that has moved it into a container can still focus it later (e.g. autofocus on tab activation) without keeping a reference to the area itself.
    pub fn focus_handle(&self) -> focus::FocusHandle {
        focus::handle(self.id)
    }

    /// The id this area holds in the tab order, for a box that frames it and has to know when it holds the keyboard. See [`Input::focus_id`](crate::Input::focus_id).
    pub fn focus_id(&self) -> FocusId {
        self.id
    }

    /// `text` in the rows this area draws it in, at the width it was laid out at.
    fn placed<'a>(&self, text: &'a str, style: &TextStyle) -> Placed<'a> {
        place(
            &self.rows,
            text,
            style,
            laid_width(self.leaf.node, self.leaf.rect.get()),
        )
    }

    /// The current caret byte offset, clamped to the text and snapped to a char boundary.
    fn caret_at(&self, text: &str) -> usize {
        char_boundary_at_or_before(text, self.caret.get())
    }

    /// The selected byte range, low end first, or `None` when nothing is selected.
    fn selection(&self, text: &str) -> Option<(usize, usize)> {
        let caret = self.caret_at(text);
        let anchor = char_boundary_at_or_before(text, self.anchor.get()?);
        (anchor != caret).then(|| (anchor.min(caret), anchor.max(caret)))
    }

    fn selected_text(&self, text: &str) -> Option<String> {
        self.selection(text)
            .map(|(from, to)| text[from..to].to_string())
    }

    /// Removes the selection from `text` and reports where the caret lands. Every edit runs through this first, so typing over a selection replaces it.
    fn take_selection(&self, text: &mut String) -> Option<usize> {
        let (from, to) = self.selection(text)?;
        text.replace_range(from..to, "");
        Some(from)
    }

    /// Applies a key while focused, editing the bound signal and/or moving the caret. Returns whether the key was consumed.
    ///
    /// Home and End, and Command with the horizontal arrows on a Mac, go to the ends of the row on screen; the primary modifier with Home or End, and Command with the vertical arrows on a Mac, go to the ends of the text.
    fn edit(&mut self, key: &Key, mods: &ModifiersState, style: &TextStyle) -> EventResult {
        // Before the key is even read; see `Input::edit`.
        self.blink.wake();
        let goal_x = self.goal_x.take();
        let mut text = self.value.get();
        let mut caret = self.caret_at(&text);
        let mut affinity = Affinity::Downstream;
        let mut changed = false;
        // Resolved after the match so each arm can still read the selection it is replacing.
        let anchor = if mods.is_shift {
            Some(self.anchor.get().unwrap_or(caret))
        } else {
            None
        };
        let command = keyboard_convention() == KeyboardConvention::Mac && mods.is_meta;
        match key {
            Key::Char('a') | Key::Char('A') if mods.is_primary() => {
                self.anchor.set(Some(0));
                self.caret.set(text.len());
                self.affinity.set(Affinity::Downstream);
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
                caret = self.take_selection(&mut text).unwrap_or(caret);
                changed = true;
            }
            // An editor is exactly where a multi-line paste belongs, so newlines are kept.
            Key::Char('v') | Key::Char('V') if mods.is_primary() => {
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
            // Any other chord is a shortcut, not text.
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
            Key::Named(NamedKey::Home) if mods.is_primary() => caret = 0,
            Key::Named(NamedKey::End) if mods.is_primary() => caret = text.len(),
            Key::Named(NamedKey::ArrowUp) if command => caret = 0,
            Key::Named(NamedKey::ArrowDown) if command => caret = text.len(),
            Key::Named(NamedKey::ArrowLeft) if command => {
                caret = self.row_start(&text, caret, style)
            }
            Key::Named(NamedKey::Home) => caret = self.row_start(&text, caret, style),
            Key::Named(NamedKey::ArrowRight) if command => {
                (caret, affinity) = self.row_end(&text, caret, style);
            }
            Key::Named(NamedKey::End) => (caret, affinity) = self.row_end(&text, caret, style),
            Key::Named(NamedKey::ArrowLeft) => caret = prev_boundary(&text, caret),
            Key::Named(NamedKey::ArrowRight) => caret = next_boundary(&text, caret),
            Key::Named(NamedKey::ArrowUp) => {
                (caret, affinity) = self.step_row(&text, caret, style, goal_x, -1);
            }
            Key::Named(NamedKey::ArrowDown) => {
                (caret, affinity) = self.step_row(&text, caret, style, goal_x, 1);
            }
            Key::Named(NamedKey::Escape) => {
                focus::release(self.id);
                return EventResult::Handled;
            }
            _ => return EventResult::Ignored,
        }
        if changed {
            self.value.set(text);
        }
        self.caret.set(caret);
        self.affinity.set(affinity);
        // An anchor that caught up with the caret is no selection at all.
        self.anchor.set(anchor.filter(|a| *a != caret));
        EventResult::Handled
    }

    fn row_start(&self, text: &str, caret: usize, style: &TextStyle) -> usize {
        let placed = self.placed(text, style);
        placed
            .lines
            .row(placed.row_of(caret, self.affinity.get()))
            .start
    }

    /// The end of the caret's row, and the affinity that keeps the caret on it rather than at the start of the row it wraps into.
    fn row_end(&self, text: &str, caret: usize, style: &TextStyle) -> (usize, Affinity) {
        let placed = self.placed(text, style);
        let row = placed.row_of(caret, self.affinity.get());
        let affinity = if placed.lines.wraps_after(row) {
            Affinity::Upstream
        } else {
            Affinity::Downstream
        };
        (placed.lines.row(row).end, affinity)
    }

    /// The caret one row up (`-1`) or down (`1`) at the x the run of vertical moves began at, or the start or end of the text past the first or last row.
    fn step_row(
        &mut self,
        text: &str,
        caret: usize,
        style: &TextStyle,
        goal_x: Option<f32>,
        by: isize,
    ) -> (usize, Affinity) {
        let placed = self.placed(text, style);
        let affinity = self.affinity.get();
        let row = placed.row_of(caret, affinity);
        let x = goal_x.unwrap_or_else(|| placed.caret_x(caret, affinity));
        self.goal_x = Some(x);
        match row.checked_add_signed(by) {
            Some(next) if next < placed.lines.len() => placed.offset_at(next, x),
            Some(_) => (text.len(), Affinity::Downstream),
            None => (0, Affinity::Downstream),
        }
    }

    /// Follows the input method composing into the area. See [`Input`](crate::Input).
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

    /// Types `inserted` at the caret, in place of the selection, its lines and tabs included.
    fn insert(&self, inserted: &str) {
        let inserted: String = inserted
            .replace("\r\n", "\n")
            .chars()
            .map(|c| if c == '\r' { '\n' } else { c })
            .filter(|c| matches!(c, '\n' | '\t') || !c.is_control())
            .collect();
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
}

/// `text` laid out in `rows` at `width`, the width the area was laid out at.
fn place<'a>(rows: &Rows, text: &'a str, style: &TextStyle, width: f32) -> Placed<'a> {
    let width = rows.wrap_width(width);
    Placed {
        text,
        lines: rows.lines(text, style, width),
        flat: style.clone().with_text_wrap(TextWrap::NoWrap),
        width,
    }
}

/// How many rows tall the area draws its text, with the composition in progress drawn into it: one that runs onto a new row grows the area as typing would.
fn drawn_rows(
    rows: &Rows,
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
            rows.shown_rows(&composed.text, style, width)
        }
        None => value.with(|text| rows.shown_rows(text, style, width)),
    }
}

/// The lines under a composition, one stretch per row it is drawn across.
fn composing_marks(
    placed: &Placed<'_>,
    composed: &Composed,
    line_h: f32,
    style: &TextStyle,
) -> RenderNode {
    let span = &composed.span;
    let first = placed.row_of(span.start, Affinity::Downstream);
    let last = placed.row_of(span.end, Affinity::Upstream);
    let mut marks = Vec::new();
    for row in first..=last {
        let range = placed.lines.row(row);
        let on_row = |part: &Range<usize>| {
            let (from, to) = (part.start.max(range.start), part.end.min(range.end));
            (from <= to).then(|| (placed.x_at(row, from), placed.x_at(row, to)))
        };
        let Some((x0, x1)) = on_row(span) else {
            continue;
        };
        let converting = composed.converting.as_ref().and_then(on_row);
        let bottom = (row + 1) as f32 * line_h;
        marks.extend(composition::underlines(
            x0,
            x1,
            converting,
            bottom,
            style.color,
        ));
    }
    RenderNode::group(marks)
}

/// The width the area's rows wrap at: as wide as layout made it before rounding took any off, since that is the width it was measured at and a row measured to fit must not break again when drawn.
fn laid_width(node: NodeId, rect: Rect) -> f32 {
    if rect.width <= 0.0 {
        return rect.width;
    }
    layout_reactive::unrounded_width(node).map_or(rect.width, |unrounded| rect.width.max(unrounded))
}

impl Component for TextArea {
    fn view(&self) -> RenderNode {
        let r = self.leaf.rect.get();
        let text = self.value.get();
        let style = (self.style)();
        let line_h = crate::text_metrics::line_box(&style);
        let focused = focus::is_focused(self.id);
        let caret = self.caret_at(&text);
        let composed = focused
            .then(|| self.composition.get())
            .flatten()
            .map(|preedit| composition::compose(&text, caret, &preedit));
        let drawn = composed.as_ref().map_or(text.as_str(), |c| c.text.as_str());
        let placed = self.placed(drawn, &style);
        let full = Rect {
            x: 0.0,
            y: 0.0,
            width: placed.width.unwrap_or(NO_WRAP_WIDTH),
            height: r.height.max(line_h),
        };
        let placeholder = self.rows.placeholder.borrow();
        let text_node = if drawn.is_empty() && !placeholder.is_empty() {
            let mut ph_style = style.clone();
            ph_style.color = style.color.faded(crate::input::PLACEHOLDER_OPACITY);
            RenderNode::text(placeholder.clone(), full, ph_style)
        } else {
            RenderNode::text(Arc::<str>::from(drawn), full, style.clone())
        };

        if !focused {
            self.drawn_caret.set(None);
            return self
                .leaf
                .at_layout_position_as(|| self.semantics(), text_node);
        }
        // One rect per row the selection spans: a selection that wraps is not one box, and drawing it as one would paint over the margin the text does not occupy.
        let highlight = match composed {
            Some(_) => None,
            None => self.selection(&text),
        }
        .map(|(from, to)| {
            let first = placed.row_of(from, Affinity::Downstream);
            let last = placed.row_of(to, Affinity::Upstream);
            let fill = style.color.faded(0.25);
            let bands = (first..=last).map(|row| {
                let range = placed.lines.row(row);
                let x0 = placed.x_at(row, if row == first { from } else { range.start });
                let x1 = if row == last {
                    placed.x_at(row, to)
                } else if placed.lines.wraps_after(row) {
                    placed.x_at(row, range.end)
                } else {
                    // A sliver on an empty line keeps the selection continuous rather than a gap the eye reads as its end.
                    placed.x_at(row, range.end).max(x0 + line_h * 0.35)
                };
                RenderNode::rect(
                    Rect {
                        x: x0,
                        y: row as f32 * line_h,
                        width: (x1 - x0).max(1.0),
                        height: line_h,
                    },
                    RectStyle::default().with_fill(fill),
                )
            });
            RenderNode::group(bands)
        });
        let composing = composed
            .as_ref()
            .map(|c| composing_marks(&placed, c, line_h, &style));
        let (caret_at, affinity) = match &composed {
            Some(c) => (c.anchor, Affinity::Downstream),
            None => (caret, self.affinity.get()),
        };
        let caret_rect = Rect {
            x: placed.caret_x(caret_at, affinity),
            y: placed.row_of(caret_at, affinity) as f32 * line_h,
            width: CARET_WIDTH,
            height: line_h,
        };
        self.drawn_caret.set(Some(TextInput {
            field: self.id,
            caret: caret_rect,
            secret: false,
        }));
        // Read here and nowhere else, so this is what subscribes the area's view to the blink.
        let caret_node = composed
            .as_ref()
            .is_none_or(|c| c.caret.is_some())
            .then(|| {
                let lit = style.color.faded(self.blink.opacity());
                RenderNode::rect(caret_rect, RectStyle::default().with_fill(lit))
            });
        let layers = highlight
            .into_iter()
            .chain(std::iter::once(text_node))
            .chain(composing)
            .chain(caret_node);
        self.leaf.at_layout_position_editing(
            || self.semantics(),
            Some(caret_rect),
            RenderNode::group(layers),
        )
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
                if !rect.contains(*x as f32, *y as f32) {
                    return EventResult::Ignored;
                }
                focus::request_from_pointer(self.id);
                self.goal_x = None;
                let style = (self.style)();
                let line_h = crate::text_metrics::line_box(&style);
                // Read the text out before setting the caret: a `set` inside a `with` closure would re-borrow the runtime.
                let text = self.value.get();
                let placed = self.placed(&text, &style);
                let local_y = (*y as f32 - rect.y).max(0.0);
                let local_x = (*x as f32 - rect.x).max(0.0);
                let row = ((local_y / line_h).floor() as usize).min(placed.lines.len() - 1);
                let (at, affinity) = placed.offset_at(row, local_x);
                // Extends from where the selection already starts, the only pointer gesture available until a drag can be tracked.
                if crate::keyboard::modifiers().is_shift {
                    let from = self.anchor.get().unwrap_or_else(|| self.caret.get());
                    self.anchor.set(Some(from));
                } else {
                    self.anchor.set(None);
                }
                self.caret.set(at);
                self.affinity.set(affinity);
                EventResult::Handled
            }
            Event::Ime(ime) if focus::is_key_target(self.id) => self.compose(ime),
            // The input method owns the keys while it composes, whichever reach the area anyway.
            Event::KeyPressed { .. }
                if focus::is_key_target(self.id) && self.composition.is_active() =>
            {
                EventResult::Handled
            }
            Event::KeyPressed { key, modifiers, .. } if focus::is_key_target(self.id) => {
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
        if let Some(_home) = self.home.enter() {
            focus::unregister(self.id);
        }
    }
}

impl_leaf_widget!(TextArea);

/// Byte offset `i` clamped to `s` and moved back onto a char boundary, for an offset kept while the text changed under it.
fn char_boundary_at_or_before(s: &str, i: usize) -> usize {
    let mut j = i.min(s.len());
    while j > 0 && !s.is_char_boundary(j) {
        j -= 1;
    }
    j
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
#[path = "text_area_test.rs"]
mod tests;
