//! The pieces a list-shaped control is made of — [`item`], [`separator`], [`group`] — and the [`ListContext`]
//! their parent provides so they can be written as siblings instead of passed in as data.
//!
//! This is the compound-component half of the catalogue. A `menu` used to take `items: Vec<&'static str>`,
//! which is the whole of what a string can say: no row could be disabled, carry a shortcut, show a tick, or
//! be anything but one line of text. Widening that prop would have meant a struct per feature and a call site
//! written in Rust rather than in markup.
//!
//! What replaces it is the arrangement Radix reaches for: the parent publishes a context, and each piece
//! reads it. The piece is then an ordinary component with ordinary props, and `disabled:$x` on a row means
//! what it means on any other widget. See [`Children`] for the ordering problem this had to solve first —
//! a child is an argument, so without it a row would be built before the menu it belongs to exists.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use layout_core::{AlignItems, JustifyContent, LayoutError, LayoutStyle};
use reactive_core::RwSignal;
use renderer_core::{RectStyle, ShapeStyle, TextStyle};
use ui_core::{LayoutItem, Slots, StyledContainer, Text, box_item, use_context};

use crate::shared;

/// What a row needs to know about the list it is in, and what the list needs to learn from its rows.
///
/// Both directions, and that is the point: the parent hands down how to commit and how to paint, and each row
/// hands back whether the keyboard may stop on it. Neither half is expressible with props alone — the parent
/// cannot write props for rows it has not seen, and a row cannot reach a parent it was built before.
#[derive(Clone)]
pub struct ListContext(Rc<ListState>);

struct ListState {
    pick: Rc<dyn Fn(u32)>,
    /// The keyboard cursor. Deliberately not the selection: moving it commits nothing.
    highlighted: RwSignal<Option<u32>>,
    /// The bound choice, for a list that has one (`select`) rather than one-shot actions (`menu`).
    selected: Option<RwSignal<u32>>,
    color: shared::ReactiveColor,
    rows: RefCell<Vec<Row>>,
    /// A cursor asked for before there was anything to put it on. Opening with the down arrow happens in a
    /// key handler, and the rows are built on the flush after it returns, so the request has to wait for them.
    seed: Cell<bool>,
}

struct Row {
    /// Whether the keyboard cursor may land here — a closure, not a flag, so a row that becomes disabled
    /// while the panel is open stops being a stop straight away instead of at the next rebuild.
    reachable: Rc<dyn Fn() -> bool>,
}

impl ListContext {
    pub(crate) fn new(
        pick: Rc<dyn Fn(u32)>,
        highlighted: RwSignal<Option<u32>>,
        selected: Option<RwSignal<u32>>,
        color: shared::ReactiveColor,
    ) -> Self {
        Self(Rc::new(ListState {
            pick,
            highlighted,
            selected,
            color,
            rows: RefCell::new(Vec::new()),
            seed: Cell::new(false),
        }))
    }

    /// Puts the cursor on the first row that can take it, as soon as such a row exists. See [`ListState::seed`].
    pub(crate) fn seed_cursor(&self) {
        self.0.seed.set(true);
    }

    /// Drops the row registry ahead of a rebuild. The panel is remade on every open, and rows that
    /// accumulated across opens would leave the keyboard walking through positions that no longer exist.
    pub(crate) fn begin(&self) {
        self.0.rows.borrow_mut().clear();
    }

    /// Registers a row and hands it the index it will commit. Called by each piece as it builds itself, so
    /// the order is the order they are written in.
    fn claim(&self, reachable: Rc<dyn Fn() -> bool>) -> u32 {
        let index = {
            let mut rows = self.0.rows.borrow_mut();
            rows.push(Row {
                reachable: reachable.clone(),
            });
            rows.len() as u32 - 1
        };
        // Outside the borrow: `set` flushes, and an effect that reads the cursor would re-enter the registry.
        if self.0.seed.get() && reachable() {
            self.0.seed.set(false);
            self.0.highlighted.set(Some(index));
        }
        index
    }

    pub(crate) fn len(&self) -> u32 {
        self.0.rows.borrow().len() as u32
    }

    /// Moves the cursor `delta` places, passing over rows it may not stop on and wrapping at the ends.
    ///
    /// Skipping rather than stopping is what makes a disabled row and a separator the same thing to the
    /// keyboard: both are in the list and neither is a destination. Returns `None` when nothing in the list
    /// can be reached at all, which is the only case where an arrow key does nothing.
    pub(crate) fn step(&self, from: Option<u32>, delta: i64) -> Option<u32> {
        let n = self.len() as i64;
        if n == 0 {
            return None;
        }
        let start = match from {
            Some(i) => i as i64,
            // Downward starts above the top and upward below the bottom, so the first step lands on the end
            // the user came from rather than one past it.
            None if delta > 0 => -1,
            None => n,
        };
        let rows = self.0.rows.borrow();
        // At most one lap: a list whose every row is unreachable must not spin.
        (1..=n).find_map(|k| {
            let i = (start + delta.signum() * k).rem_euclid(n);
            (rows[i as usize].reachable)().then_some(i as u32)
        })
    }

    /// The first row the keyboard may stop on, from whichever end `delta` starts at. What Home and End mean,
    /// and where a fresh cursor lands.
    pub(crate) fn edge(&self, delta: i64) -> Option<u32> {
        self.step(None, delta)
    }

    pub(crate) fn pick(&self, index: u32) {
        (self.0.pick)(index);
    }
}

/// One row of a list — an action in a menu, a choice in a select.
///
/// Outside any list it still builds, as a plain row that fires its own `on_press`. That is deliberate: the
/// mistake of writing an item somewhere it has no parent is made in markup, and a component that panicked on
/// it would report a markup error as a crash in Rust nobody wrote.
pub struct ItemProps {
    pub label: Box<dyn Fn() -> String>,
    /// Greys the row, takes it out of the keyboard's path, and stops it committing anything.
    pub disabled: Box<dyn Fn() -> bool>,
    /// Draws a tick on the row — for a menu of toggles, where the row is a *state* rather than an action.
    ///
    /// `Option` rather than a closure defaulting to `false`, so the row can tell "no tick here" from "a tick
    /// that is currently off". It reserves the column either way once it is written, because a tick column
    /// that appeared with the first tick would shift every label beside it.
    pub checked: Option<Box<dyn Fn() -> bool>>,
    /// Trailing text, quiet: a keyboard shortcut, a count, a units suffix. `Option` for the same reason as
    /// [`checked`](Self::checked).
    pub hint: Option<Box<dyn Fn() -> String>>,
    /// Fired when the row is committed, on top of whatever the enclosing list does with the index.
    pub on_press: Option<Box<dyn Fn()>>,
}

impl Default for ItemProps {
    fn default() -> Self {
        Self {
            label: Box::new(String::new),
            disabled: Box::new(|| false),
            checked: None,
            hint: None,
            on_press: None,
        }
    }
}

pub fn item(props: ItemProps, children: Slots) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let ItemProps {
        label,
        disabled,
        checked,
        hint,
        on_press,
    } = props;
    let disabled: Rc<dyn Fn() -> bool> = Rc::from(disabled);
    let list = use_context::<ListContext>();
    let index = list.as_ref().map(|ctx| {
        let disabled = disabled.clone();
        ctx.claim(Rc::new(move || !disabled()))
    });

    let mut content: Vec<Box<dyn LayoutItem>> = Vec::new();
    if let Some(checked) = checked {
        content.push(box_item(Text::auto(
            move || {
                if checked() {
                    "✓".into()
                } else {
                    String::new()
                }
            },
            LayoutStyle::new().width(14.0),
            || TextStyle::new(shared::font_size(), shared::ink()),
        )?));
    }
    // Markup children are the row's content when there are any: an icon beside a label, two lines, a swatch.
    // The `label` prop is the one-line shorthand for exactly the case that needs nothing else.
    let mut given = children;
    let supplied = given.take_default();
    if supplied.is_empty() {
        content.push(box_item(Text::auto(label, LayoutStyle::new(), || {
            TextStyle::new(shared::font_size(), shared::ink()).with_no_wrap(true)
        })?));
    } else {
        content.extend(supplied);
    }
    if let Some(hint) = hint {
        content.push(box_item(Text::auto(hint, LayoutStyle::new(), || {
            TextStyle::new(shared::font_size() * 0.9, shared::ink().with_alpha(0.65))
                .with_no_wrap(true)
        })?));
    }

    let row_style = {
        let list = list.clone();
        move |_r| match (&list, index) {
            (Some(ctx), Some(i)) => ctx.row_style(i),
            _ => RectStyle::default(),
        }
    };
    let hover_style = {
        let list = list.clone();
        move |_r| match &list {
            Some(ctx) => ctx.hover_style(),
            None => RectStyle::default(),
        }
    };
    let commit = {
        let list = list.clone();
        move || {
            if let (Some(ctx), Some(i)) = (&list, index) {
                ctx.pick(i);
            }
            if let Some(cb) = &on_press {
                cb();
            }
        }
    };

    let row = StyledContainer::new(row_layout(), row_style, content)?
        .on_hover_style(hover_style)
        .disabled(move || disabled())
        .on_press(commit);
    Ok(box_item(row))
}

/// A rule between groups of rows. Registered like a row so it takes a position in the list, and unreachable
/// so the keyboard passes straight over it.
pub fn separator() -> Result<Box<dyn LayoutItem>, LayoutError> {
    if let Some(ctx) = use_context::<ListContext>() {
        ctx.claim(Rc::new(|| false));
    }
    let rule = StyledContainer::new(
        LayoutStyle::new()
            .height(1.0)
            .margin_vertical(shared::spacing() * 0.25),
        |_r| RectStyle::default().with_fill(shared::border()),
        vec![],
    )?;
    Ok(box_item(rule))
}

/// A heading over the rows beneath it. Also unreachable: it names a group, it is not one of its members.
pub struct GroupProps {
    pub label: Box<dyn Fn() -> String>,
}

impl Default for GroupProps {
    fn default() -> Self {
        Self {
            label: Box::new(String::new),
        }
    }
}

pub fn group(props: GroupProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    if let Some(ctx) = use_context::<ListContext>() {
        ctx.claim(Rc::new(|| false));
    }
    let text = Text::auto(props.label, LayoutStyle::new(), || {
        TextStyle::new(shared::font_size() * 0.85, shared::ink().with_alpha(0.65))
            .with_no_wrap(true)
    })?;
    let heading = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .padding_horizontal(10.0)
            .padding_vertical(shared::spacing() * 0.25),
        |_r| RectStyle::default(),
        vec![box_item(text)],
    )?;
    Ok(box_item(heading))
}

fn row_layout() -> LayoutStyle {
    LayoutStyle::new()
        .flex_row()
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::SPACE_BETWEEN)
        .height(crate::dropdown::ROW_HEIGHT)
        .padding_horizontal(10.0)
        .gap(8.0)
}

impl ListContext {
    fn row_style(&self, index: u32) -> RectStyle {
        // The keyboard cursor wears the hover paint on purpose: it says the same thing — this is the row a
        // commit would take — and a look of its own would have the list report two different places at once.
        if self.0.highlighted.get() == Some(index) {
            return self.hover_style();
        }
        let is_selected = self.0.selected.as_ref().is_some_and(|s| s.get() == index);
        crate::dropdown::option_row_style(is_selected, self.0.color.as_ref())
    }

    fn hover_style(&self) -> RectStyle {
        crate::dropdown::option_row_hover_style(self.0.color.as_ref())
    }
}
