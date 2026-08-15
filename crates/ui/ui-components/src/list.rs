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
use std::time::{Duration, Instant};

use layout_core::{AlignItems, JustifyContent, LayoutError, LayoutStyle};
use reactive_core::RwSignal;
use renderer_core::{RectStyle, ShapeStyle, TextStyle};
use ui_core::focus::Role;
use ui_core::{Container, LayoutItem, Slots, StyledContainer, Text, box_item, use_context};

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
    search: RefCell<Search>,
    /// Set while the list is being walked for what its rows *say* rather than for the rows. See
    /// [`ListContext::declare`].
    declaring: Cell<bool>,
}

struct Row {
    /// Whether the keyboard cursor may land here — a closure, not a flag, so a row that becomes disabled
    /// while the panel is open stops being a stop straight away instead of at the next rebuild.
    reachable: Rc<dyn Fn() -> bool>,
    /// What type-ahead matches against, for the same reason a closure: a row whose label tracks a signal is
    /// findable by what it says now. Empty for a piece that is not a destination, and for a row written with
    /// markup children instead of a label — there is no text to match, so nothing claims to match it.
    label: Rc<dyn Fn() -> String>,
}

/// How long a pause ends a type-ahead query. Past it the next character starts a fresh search rather than
/// extending one the user has stopped thinking about — the same second every native list allows.
const SEARCH_TIMEOUT: Duration = Duration::from_millis(1000);

/// The type-ahead query and when it was last typed into.
#[derive(Default)]
struct Search {
    query: String,
    typed_at: Option<Instant>,
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
            search: RefCell::new(Search::default()),
            declaring: Cell::new(false),
        }))
    }

    /// Walks `rows` for their metadata alone: every piece registers what it is and hands back a bare node
    /// instead of building itself.
    ///
    /// This exists for one thing a bound list needs and a menu does not. A `select`'s trigger has to name the
    /// current choice *before the panel has ever been opened*, and the rows only exist once it has — so the
    /// labels could not be where that name came from, and the component kept a flat `options` prop instead.
    /// Asking the rows what they say, without asking them to be rows, is what removes that.
    pub(crate) fn declare(&self, rows: &ui_core::Children) -> Result<(), LayoutError> {
        self.begin();
        self.0.declaring.set(true);
        let declared = rows.build_with(self.clone());
        self.0.declaring.set(false);
        // Nothing frees a layout node on drop, and `remove` does not reach descendants — which is exactly why
        // the pieces hand back a childless node rather than a built row.
        for item in declared?.take_default() {
            ui_core::remove_node(item.layout_node());
        }
        Ok(())
    }

    fn is_declaring(&self) -> bool {
        self.0.declaring.get()
    }

    /// What the row at `index` says right now, for a trigger that names the chosen one.
    pub(crate) fn label_of(&self, index: u32) -> Option<String> {
        // Cloned out of the borrow before it is called: a label may read a signal, and reading one can flush
        // effects that come back through the registry.
        let label = self.0.rows.borrow().get(index as usize)?.label.clone();
        Some(label())
    }

    /// Puts the cursor on the first row that can take it, as soon as such a row exists. See [`ListState::seed`].
    pub(crate) fn seed_cursor(&self) {
        self.0.seed.set(true);
    }

    /// Drops the row registry ahead of a rebuild. The panel is remade on every open, and rows that
    /// accumulated across opens would leave the keyboard walking through positions that no longer exist.
    pub(crate) fn begin(&self) {
        self.0.rows.borrow_mut().clear();
        // A fresh open is a fresh search: a query left over from the last one would make the first keystroke
        // land somewhere the user never typed towards.
        *self.0.search.borrow_mut() = Search::default();
    }

    /// Registers a row and hands it the index it will commit. Called by each piece as it builds itself, so
    /// the order is the order they are written in.
    fn claim(&self, reachable: Rc<dyn Fn() -> bool>, label: Rc<dyn Fn() -> String>) -> u32 {
        let index = {
            let mut rows = self.0.rows.borrow_mut();
            rows.push(Row {
                reachable: reachable.clone(),
                label,
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

    /// Registers a piece that holds a position without being one — a rule, a heading. It takes an index so the
    /// rows around it keep the ones they commit with, and the keyboard passes straight over it.
    fn claim_unreachable(&self) {
        self.claim(Rc::new(|| false), Rc::new(String::new));
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

    /// Whether a type-ahead query is still running, which is what makes a space a character rather than a key.
    pub(crate) fn is_searching(&self) -> bool {
        let search = self.0.search.borrow();
        !search.query.is_empty()
            && search
                .typed_at
                .is_some_and(|t| t.elapsed() < SEARCH_TIMEOUT)
    }

    /// Extends the type-ahead query with `c` and returns the row it now names, or `None` when nothing matches.
    ///
    /// Two behaviours that look like special cases and are the whole feature. **A repeated character cycles**:
    /// `d`, `d`, `d` walks the rows starting with *d* rather than searching for "ddd", which no label has, and
    /// it is the only way to reach the second of two rows sharing a first letter. **A refined query holds
    /// still**: typing `de` after `d` may keep the row `d` landed on, because the user is narrowing towards it
    /// rather than asking for the next one. That is why a one-character needle skips the current row and a
    /// longer one does not.
    pub(crate) fn type_ahead(&self, c: char, from: Option<u32>) -> Option<u32> {
        let query = self.extend_query(c);
        // Read off the query rather than off `c`, which is still in the case the user typed it in.
        let first = query.chars().next()?;
        let repeated = query.chars().count() > 1 && query.chars().all(|q| q == first);
        let needle: &str = if repeated {
            &query[..first.len_utf8()]
        } else {
            &query
        };
        let skip_current = needle.chars().count() == 1;

        let rows = self.0.rows.borrow();
        let n = rows.len();
        if n == 0 {
            return None;
        }
        // From wherever the cursor is, once round: a search that found nothing must not spin, and one that
        // wraps has to be able to reach the rows above where it started.
        let start = from.unwrap_or(0) as usize;
        (0..n).find_map(|k| {
            let i = (start + k) % n;
            if skip_current && Some(i as u32) == from {
                return None;
            }
            let row = &rows[i];
            ((row.reachable)() && (row.label)().to_lowercase().starts_with(needle))
                .then_some(i as u32)
        })
    }

    /// Appends `c` to the query, starting a new one if the last keystroke has gone stale. Lowercased on the
    /// way in so the match is case-insensitive without lowercasing the needle once per row.
    fn extend_query(&self, c: char) -> String {
        let mut search = self.0.search.borrow_mut();
        if search
            .typed_at
            .is_none_or(|t| t.elapsed() >= SEARCH_TIMEOUT)
        {
            search.query.clear();
        }
        search.query.extend(c.to_lowercase());
        search.typed_at = Some(Instant::now());
        search.query.clone()
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
    let label: Rc<dyn Fn() -> String> = Rc::from(label);
    let list = use_context::<ListContext>();
    let index = list.as_ref().map(|ctx| {
        let disabled = disabled.clone();
        ctx.claim(Rc::new(move || !disabled()), label.clone())
    });
    if let Some(bare) = declared_placeholder(&list) {
        return bare;
    }

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
        content.push(box_item(Text::auto(
            move || label(),
            LayoutStyle::new(),
            || TextStyle::new(shared::font_size(), shared::ink()).with_no_wrap(true),
        )?));
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
    // Announced without becoming a Tab stop: the list is driven by arrows and type-ahead from the trigger, so
    // a reader has to be able to describe the open panel while Tab keeps treating it as one control.
    if list.is_some() {
        ui_core::focus::register_presented(
            ui_core::focus::next_id(),
            row.layout_node(),
            Role::MenuItem,
        );
    }
    Ok(box_item(row))
}

/// A rule between groups of rows. Registered like a row so it takes a position in the list, and unreachable
/// so the keyboard passes straight over it.
pub fn separator() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let list = use_context::<ListContext>();
    if let Some(ctx) = &list {
        ctx.claim_unreachable();
    }
    if let Some(bare) = declared_placeholder(&list) {
        return bare;
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
    let list = use_context::<ListContext>();
    if let Some(ctx) = &list {
        ctx.claim_unreachable();
    }
    if let Some(bare) = declared_placeholder(&list) {
        return bare;
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

/// What a piece returns during [`ListContext::declare`]: a childless node and nothing else — no text shaped,
/// no styles resolved, no focus or hit region registered. `None` when this is a real build.
fn declared_placeholder(
    list: &Option<ListContext>,
) -> Option<Result<Box<dyn LayoutItem>, LayoutError>> {
    list.as_ref()
        .is_some_and(ListContext::is_declaring)
        .then(|| Ok(box_item(Container::new(LayoutStyle::new(), vec![])?)))
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
