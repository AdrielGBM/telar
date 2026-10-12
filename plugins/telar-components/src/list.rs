//! The pieces a list-shaped control is made of — [`item`], [`separator`], [`group`] — and the [`ListContext`] their parent provides so they can be written as siblings instead of passed in as data.
//!
//! This is the compound-component half of the catalogue. A `menu` used to take `items: Vec<&'static str>`, which is the whole of what a string can say: no row could be disabled, carry a shortcut, show a tick, or be anything but one line of text. Widening that prop would have meant a struct per feature and a call site written in Rust rather than in markup.
//!
//! What replaces it is the arrangement Radix reaches for: the parent publishes a context, and each piece reads it. The piece is then an ordinary component with ordinary props, and `disabled:$x` on a row means what it means on any other widget. See [`Children`] for the ordering problem this had to solve first — a child is an argument, so without it a row would be built before the menu it belongs to exists.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use telar::{
    Accessible, AlignItems, BorderRadius, Children, Color, Container, JustifyContent, LayoutError,
    LayoutItem, LayoutStyle, Margin, NodeId, Props, Reactive, RectStyle, RwSignal, ShapeStyle,
    StyledContainer, Text, TextWrap, box_item,
    focus::{FocusId, Role},
    use_context,
};

use crate::shared;
use crate::type_ahead::TypeAhead;

/// A row's hint and a group's heading, as shares of the label they sit with, at the alpha that makes them read as secondary without becoming a second colour a theme has to answer for.
const HINT_RATIO: f32 = 0.9;
const HEADING_RATIO: f32 = 0.85;

/// The tokens of the rows a list draws — the items of a menu and a select, the commands of the palette, the rows of a context menu — which an application's theme supplies through a `#[theme(extension)]` field.
///
/// Every field left `None` follows the role named beside it, read when the row is laid out or painted, so every list in an application keeps one row height and one corner unless its theme says otherwise.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ListTokens {
    /// Height of a row at the regular control size, scaled with the control size in force. `None` is [`metrics::row_height`](crate::metrics::row_height).
    pub row_height: Option<f32>,
    /// Corner of the wash behind a row. `None` is the theme's `radius_item`.
    pub radius: Option<f32>,
    /// Space between a row's edge and what it says at the regular control size, scaled with the control size in force. `None` is [`metrics::gap_md`](crate::metrics::gap_md).
    pub pad_x: Option<f32>,
}

impl ListTokens {
    /// The tokens of the theme in force, subscribing the caller to it.
    pub fn current() -> Self {
        telar::use_theme_extension::<Self>()
    }

    /// In whole pixels: a virtual list places the rows it has not built by multiplying it, and a fractional height drifts off the pixels layout snaps the built ones to.
    pub fn row_height(&self) -> f32 {
        self.row_height
            .map_or_else(shared::row_height, |height| height * telar::control_scale())
            .round()
    }

    pub fn radius(&self) -> f32 {
        self.radius.unwrap_or_else(shared::radius_item)
    }

    pub fn pad_x(&self) -> f32 {
        self.pad_x
            .map_or_else(shared::gap_md, |pad| pad * telar::control_scale())
    }
}

/// `wash` as a list drawn in `color` lays it: the theme's own wash while `color` is unset, else `color` at the wash's strength, so a tinted list is as light or as strong as the theme's hover and selection.
pub(crate) fn tinted(color: &Reactive<Color>, wash: Color) -> Color {
    shared::resolve(color, || wash).with_alpha(wash.a)
}

/// A row's paint: `fill` behind it, in the row's corner.
pub(crate) fn row_paint(fill: Option<Color>) -> RectStyle {
    let rect = RectStyle::default().with_radius(BorderRadius::all(ListTokens::current().radius()));
    match fill {
        Some(fill) => rect.with_fill(fill),
        None => rect,
    }
}

/// What a row needs to know about the list it is in, and what the list needs to learn from its rows.
///
/// Both directions, and that is the point: the parent hands down how to commit and how to paint, and each row hands back whether the keyboard may stop on it. Neither half is expressible with props alone — the parent cannot write props for rows it has not seen, and a row cannot reach a parent it was built before.
#[derive(Clone)]
pub struct ListContext(Rc<ListState>);

struct ListState {
    pick: Rc<dyn Fn(u32)>,
    /// The keyboard cursor. Deliberately not the selection: moving it commits nothing.
    highlighted: RwSignal<Option<u32>>,
    /// The bound choice, for a list that has one (`select`) rather than one-shot actions (`menu`).
    selected: Option<RwSignal<u32>>,
    color: Reactive<Color>,
    rows: RefCell<Vec<Row>>,
    /// A cursor asked for before there was anything to put it on. Opening with the down arrow happens in a key handler, and the rows are built on the flush after it returns, so the request has to wait for them.
    seed: Cell<bool>,
    search: RefCell<TypeAhead>,
    /// Set while the list is being walked for what its rows *say* rather than for the rows. See [`ListContext::declare`].
    declaring: Cell<bool>,
}

struct Row {
    /// Whether the keyboard cursor may land here — a closure, not a flag, so a row that becomes disabled while the panel is open stops being a stop straight away instead of at the next rebuild.
    reachable: Reactive<bool>,
    /// What type-ahead matches against, for the same reason a closure: a row whose label tracks a signal is findable by what it says now. Empty for a piece that is not a destination, and for a row written with markup children instead of a label — there is no text to match, so nothing claims to match it.
    label: Reactive<String>,
    node: Cell<Option<NodeId>>,
    focus: Cell<Option<FocusId>>,
}

impl ListContext {
    pub(crate) fn new(
        pick: Rc<dyn Fn(u32)>,
        highlighted: RwSignal<Option<u32>>,
        selected: Option<RwSignal<u32>>,
        color: Reactive<Color>,
    ) -> Self {
        Self(Rc::new(ListState {
            pick,
            highlighted,
            selected,
            color,
            rows: RefCell::new(Vec::new()),
            seed: Cell::new(false),
            search: RefCell::new(TypeAhead::default()),
            declaring: Cell::new(false),
        }))
    }

    /// Walks `rows` for their metadata alone: every piece registers what it is and hands back a bare node instead of building itself.
    ///
    /// This exists for one thing a bound list needs and a menu does not. A `select`'s trigger has to name the current choice *before the panel has ever been opened*, and the rows only exist once it has — so the labels could not be where that name came from, and the component kept a flat `options` prop instead. Asking the rows what they say, without asking them to be rows, is what removes that.
    pub(crate) fn declare(&self, rows: &telar::Children) -> Result<(), LayoutError> {
        self.begin();
        self.0.declaring.set(true);
        let declared = rows.build_with(self.clone());
        self.0.declaring.set(false);
        for item in declared?.take_default() {
            telar::remove_node(item.layout_node());
        }
        Ok(())
    }

    fn is_declaring(&self) -> bool {
        self.0.declaring.get()
    }

    /// What the row at `index` says right now, for a trigger that names the chosen one.
    pub(crate) fn label_of(&self, index: u32) -> Option<String> {
        // Cloned out of the borrow before it is called: a label may read a signal, and reading one can flush effects that come back through the registry.
        let label = self.0.rows.borrow().get(index as usize)?.label.clone();
        Some(label.get())
    }

    /// Puts the cursor on the first row that can take it, as soon as such a row exists. See [`ListState::seed`].
    pub(crate) fn seed_cursor(&self) {
        self.0.seed.set(true);
    }

    /// Drops the row registry ahead of a rebuild. The panel is remade on every open, and rows that accumulated across opens would leave the keyboard walking through positions that no longer exist.
    pub(crate) fn begin(&self) {
        self.0.rows.borrow_mut().clear();
        // A fresh open is a fresh search: a query left over from the last one would make the first keystroke land somewhere the user never typed towards.
        *self.0.search.borrow_mut() = TypeAhead::default();
    }

    /// Registers a row and hands it the index it will commit. Called by each piece as it builds itself, so the order is the order they are written in.
    fn claim(&self, reachable: Reactive<bool>, label: Reactive<String>) -> u32 {
        let index = {
            let mut rows = self.0.rows.borrow_mut();
            rows.push(Row {
                reachable: reachable.clone(),
                label,
                node: Cell::new(None),
                focus: Cell::new(None),
            });
            rows.len() as u32 - 1
        };
        // Outside the borrow: `set` flushes, and an effect that reads the cursor would re-enter the registry.
        if self.0.seed.get() && reachable.get() {
            self.0.seed.set(false);
            self.0.highlighted.set(Some(index));
        }
        index
    }

    /// Registers a piece that holds a position without being one — a rule, a heading. It takes an index so the rows around it keep the ones they commit with, and the keyboard passes straight over it.
    fn claim_unreachable(&self) {
        self.claim(Reactive::of(|| false), Reactive::default());
    }

    fn attach(&self, index: u32, node: NodeId, focus: FocusId) {
        if let Some(row) = self.0.rows.borrow().get(index as usize) {
            row.node.set(Some(node));
            row.focus.set(Some(focus));
        }
    }

    pub(crate) fn node_of(&self, index: u32) -> Option<NodeId> {
        self.0.rows.borrow().get(index as usize)?.node.get()
    }

    /// The control a reader is pointed at for the row at `index`.
    pub(crate) fn focus_of(&self, index: u32) -> Option<FocusId> {
        self.0.rows.borrow().get(index as usize)?.focus.get()
    }

    pub(crate) fn len(&self) -> u32 {
        self.0.rows.borrow().len() as u32
    }

    /// Moves the cursor `delta` places, passing over rows it may not stop on and wrapping at the ends.
    ///
    /// Skipping rather than stopping is what makes a disabled row and a separator the same thing to the keyboard: both are in the list and neither is a destination. Returns `None` when nothing in the list can be reached at all, which is the only case where an arrow key does nothing.
    pub(crate) fn step(&self, from: Option<u32>, delta: i64) -> Option<u32> {
        let n = self.len() as i64;
        if n == 0 {
            return None;
        }
        let start = match from {
            Some(i) => i as i64,
            // Downward starts above the top and upward below the bottom, so the first step lands on the end the user came from rather than one past it.
            None if delta > 0 => -1,
            None => n,
        };
        let rows = self.0.rows.borrow();
        // At most one lap: a list whose every row is unreachable must not spin.
        (1..=n).find_map(|k| {
            let i = (start + delta.signum() * k).rem_euclid(n);
            rows[i as usize].reachable.get().then_some(i as u32)
        })
    }

    /// The first row the keyboard may stop on, from whichever end `delta` starts at. What Home and End mean, and where a fresh cursor lands.
    pub(crate) fn edge(&self, delta: i64) -> Option<u32> {
        self.step(None, delta)
    }

    pub(crate) fn pick(&self, index: u32) {
        (self.0.pick)(index);
    }

    /// Whether a type-ahead query is still running, which is what makes a space a character rather than a key.
    pub(crate) fn is_searching(&self) -> bool {
        self.0.search.borrow().is_searching()
    }

    /// Extends the type-ahead query with `c` and returns the row it now names, or `None` when nothing matches. See [`TypeAhead::extend`] for why a repeated letter cycles and a refined query holds still.
    pub(crate) fn type_ahead(&self, c: char, from: Option<u32>) -> Option<u32> {
        let needle = self.0.search.borrow_mut().extend(c);
        let rows = self.0.rows.borrow();
        needle
            .find(from.map(|i| i as usize), rows.len(), |i| {
                let row = &rows[i];
                row.reachable.get().then(|| row.label.get())
            })
            .map(|i| i as u32)
    }
}

/// One row of a list — an action in a menu, a choice in a select.
///
/// Outside any list it still builds, as a plain row that fires its own `on_press`. That is deliberate: the mistake of writing an item somewhere it has no parent is made in markup, and a component that panicked on it would report a markup error as a crash in Rust nobody wrote.
#[derive(Props)]
pub struct ItemProps {
    #[props(into, default)]
    pub label: Reactive<String>,
    /// Greys the row, takes it out of the keyboard's path, and stops it committing anything.
    #[props(into, default)]
    pub disabled: Reactive<bool>,
    /// Draws a tick on the row — for a menu of toggles, where the row is a *state* rather than an action.
    ///
    /// `Option` rather than a closure defaulting to `false`, so the row can tell "no tick here" from "a tick that is currently off". It reserves the column either way once it is written, because a tick column that appeared with the first tick would shift every label beside it.
    #[props(some, into, default)]
    pub checked: Option<Reactive<bool>>,
    /// Trailing text, quiet: a keyboard shortcut, a count, a units suffix. `Option` for the same reason as [`checked`](Self::checked).
    #[props(some, into, default)]
    pub hint: Option<Reactive<String>>,
    /// Fired when the row is committed, on top of whatever the enclosing list does with the index.
    #[props(some, default)]
    pub on_press: Option<Rc<dyn Fn()>>,
}

/// One row of a list: its content, and what a commit on it does.
pub fn item(props: ItemProps, children: Children) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let children = children.build()?;
    let ItemProps {
        label,
        disabled,
        checked,
        hint,
        on_press,
    } = props;
    let list = use_context::<ListContext>();
    let index = list.as_ref().map(|ctx| {
        let disabled = disabled.clone();
        ctx.claim(Reactive::of(move || !disabled.get()), label.clone())
    });
    if let Some(bare) = declared_placeholder(&list) {
        return bare;
    }

    let mut content: Vec<Box<dyn LayoutItem>> = Vec::new();
    let ticked = checked.clone();
    if let Some(checked) = checked {
        let tick = shared::mark(
            || shared::Mark::Check,
            move |text| match checked.get() {
                true => text.color,
                false => Color::TRANSPARENT.into(),
            },
        )?;
        content.push(box_item(
            Container::new(
                LayoutStyle::new()
                    .width(shared::icon_size())
                    .flex_shrink(0.0)
                    .flex_row()
                    .align_items(AlignItems::CENTER)
                    .justify_content(JustifyContent::CENTER),
                vec![tick],
            )?
            .a11y_hidden(),
        ));
    }
    // Markup children are the row's content when there are any; `label` is the shorthand for the case that needs nothing else.
    let mut given = children;
    let supplied = given.take_default();
    if supplied.is_empty() {
        content.push(box_item(Text::declaring(
            move || label.get(),
            LayoutStyle::new(),
            |t| shared::control_text(t, 1.0).with_text_wrap(TextWrap::NoWrap),
        )?));
    } else {
        content.extend(supplied);
    }
    if let Some(hint) = hint {
        content.push(box_item(Text::declaring(
            move || hint.get(),
            LayoutStyle::new(),
            |t| shared::quiet(t, HINT_RATIO).with_text_wrap(TextWrap::NoWrap),
        )?));
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
        move |_r| match (&list, index) {
            (Some(ctx), Some(i)) => ctx.hover_style(i),
            _ => RectStyle::default(),
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
        .styled_by(row_layout)
        .hover_style(hover_style)
        .disabled(move || disabled.get())
        .on_press(commit);
    // Announced without becoming a Tab stop: the list is driven by arrows from the trigger, so a reader can describe the open panel while Tab keeps treating it as one control.
    let row = match (&list, index) {
        (Some(ctx), Some(i)) => {
            let row = ctx.present(row, i);
            if let Some(focus) = row.focus_id() {
                ctx.attach(i, row.layout_node(), focus);
            }
            row
        }
        _ => row,
    };
    // The tick is drawn for the eye; a reader hears the state instead, and the row keeps one name whether it is ticked or not.
    let row = match ticked {
        Some(ticked) => row.toggled(move || ticked.get()),
        None => row,
    };
    Ok(box_item(row))
}

/// A rule between groups of rows. Registered like a row so it takes a position in the list, and unreachable so the keyboard passes straight over it.
#[derive(Props)]
pub struct SeparatorProps {}

/// A rule between groups of rows. Never takes the keyboard cursor.
pub fn separator(
    _props: SeparatorProps,
    _children: Children,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let list = use_context::<ListContext>();
    if let Some(ctx) = &list {
        ctx.claim_unreachable();
    }
    if let Some(bare) = declared_placeholder(&list) {
        return bare;
    }
    let rule_box = || {
        LayoutStyle::new()
            .height(shared::border_width())
            .margin(Margin::symmetric(shared::gap_xs(), 0.0))
    };
    let rule = StyledContainer::new(
        rule_box(),
        |_r| RectStyle::default().with_fill(shared::border()),
        vec![],
    )?
    .styled_by(rule_box);
    Ok(box_item(rule))
}

/// A heading over the rows beneath it. Also unreachable: it names a group, it is not one of its members.
#[derive(Props)]
pub struct GroupProps {
    #[props(into, default)]
    pub label: Reactive<String>,
}

/// A heading over the rows beneath it. Never takes the keyboard cursor.
pub fn group(props: GroupProps, _children: Children) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let list = use_context::<ListContext>();
    if let Some(ctx) = &list {
        ctx.claim_unreachable();
    }
    if let Some(bare) = declared_placeholder(&list) {
        return bare;
    }
    let text = Text::declaring(
        move || props.label.get(),
        LayoutStyle::new(),
        |t| shared::quiet(t, HEADING_RATIO).with_text_wrap(TextWrap::NoWrap),
    )?;
    let heading_box = || {
        LayoutStyle::new()
            .flex_row()
            .padding_horizontal(ListTokens::current().pad_x())
            .padding_vertical(shared::gap_xs())
    };
    let heading = StyledContainer::new(
        heading_box(),
        |_r| RectStyle::default(),
        vec![box_item(text)],
    )?
    .styled_by(heading_box);
    Ok(box_item(heading))
}

/// What a piece returns during [`ListContext::declare`]: a childless node and nothing else — no text shaped, no styles resolved, no focus or hit region registered. `None` when this is a real build.
fn declared_placeholder(
    list: &Option<ListContext>,
) -> Option<Result<Box<dyn LayoutItem>, LayoutError>> {
    list.as_ref()
        .is_some_and(ListContext::is_declaring)
        .then(|| Ok(box_item(Container::new(LayoutStyle::new(), vec![])?)))
}

fn row_layout() -> LayoutStyle {
    let tokens = ListTokens::current();
    LayoutStyle::new()
        .flex_row()
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::SPACE_BETWEEN)
        .height(tokens.row_height())
        .padding_horizontal(tokens.pad_x())
        .gap(shared::gap_md())
}

impl ListContext {
    fn present(&self, row: StyledContainer, index: u32) -> StyledContainer {
        match self.0.selected {
            Some(selected) => row
                .presented(Role::Option)
                .toggled(move || selected.get() == index),
            None => row.presented(Role::MenuItem),
        }
    }

    fn is_selected(&self, index: u32) -> bool {
        self.0.selected.as_ref().is_some_and(|s| s.get() == index)
    }

    /// The chosen row keeps the selection's wash whatever else is on it, so the cursor or the pointer passing over it never makes it look less chosen than it is.
    fn row_style(&self, index: u32) -> RectStyle {
        // The keyboard cursor wears the hover paint on purpose: it says the same thing, and a look of its own would have the list report two different places at once.
        let wash = if self.is_selected(index) {
            Some(shared::held_wash())
        } else if self.0.highlighted.get() == Some(index) {
            Some(shared::hover_wash())
        } else {
            None
        };
        row_paint(wash.map(|wash| tinted(&self.0.color, wash)))
    }

    fn hover_style(&self, index: u32) -> RectStyle {
        let wash = if self.is_selected(index) {
            shared::held_wash()
        } else {
            shared::hover_wash()
        };
        row_paint(Some(tinted(&self.0.color, wash)))
    }
}

#[cfg(test)]
#[path = "list_test.rs"]
mod tests;
