//! [`virtual_list`]: a long list of rows to choose one from, walked from the keyboard, with only the rows on screen built.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use telar::{
    Accessible, AlignItems, Children, Clip, ClippedItem, Color, Key, KeyNav, KeyNavMove,
    LayoutError, LayoutItem, LayoutScrollArea, LayoutStyle, Memo, NamedKey, Props, Reactive,
    RectStyle, RwSignal, ScrollViewport, StyledContainer, Text, TextWrap, VirtualList, box_item,
    focus::{self, FocusId, Role, SetPosition},
    key_nav_apply, memo, signal,
};

use crate::list::ListTokens;
use crate::rows::{BuiltRows, RowState, cursor_row_paint, reveal_band};
use crate::shared;
use crate::type_ahead::TypeAhead;

const OVERSCAN: usize = 4;

/// One row of a [`virtual_list`], as data: the id the selection names it by, what it says, and quiet trailing text.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ListRow {
    /// Unique among the list's rows.
    pub id: Arc<str>,
    pub label: Arc<str>,
    /// Quiet trailing text: a count, a size, a date.
    pub detail: Option<Arc<str>>,
    /// Listed, but passed over by the keyboard and never chosen.
    pub disabled: bool,
}

impl ListRow {
    pub fn new(id: impl Into<Arc<str>>, label: impl Into<Arc<str>>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            detail: None,
            disabled: false,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<Arc<str>>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

/// What [`VirtualListProps::on_select`] is called with.
pub type RowHandler = Rc<dyn Fn(&ListRow)>;

/// What [`VirtualListProps::row`] builds a row's content with.
pub type RowBuilder = Rc<dyn Fn(&ListRow) -> Result<Box<dyn LayoutItem>, LayoutError>>;

/// A list of any length to choose one row from, read by a screen reader as a list box.
///
/// It fills the height its parent gives it and scrolls within it, building only the rows on screen, so ten thousand rows cost what a screenful does. Every row stands at [`ListTokens::row_height`], which is what lets it place the rows it has not built.
///
/// The keyboard walks a cursor over the rows — the arrows, Home and End, Page Up and Page Down, and type-ahead on the rows' labels — and Enter, Space or a press chooses the row it is on. A reader hears the row the cursor is on as it moves.
#[derive(Props)]
pub struct VirtualListProps {
    #[props(into, default)]
    pub items: Reactive<Vec<ListRow>>,
    /// The bound selection, by row id. `None` (the default) is uncontrolled.
    #[props(some, into, default)]
    pub selected: Option<RwSignal<Option<Arc<str>>>>,
    /// Fired with the row a press, Enter or Space chose.
    #[props(some, default)]
    pub on_select: Option<RowHandler>,
    /// Builds what a row shows, in place of its label and detail. The row itself — its height, its wash and what a reader hears it as — stays the list's.
    #[props(some, default)]
    pub row: Option<RowBuilder>,
    /// What a reader calls the list.
    #[props(some, into, default)]
    pub label: Option<Reactive<String>>,
    /// Accent for the selection and the keyboard cursor. `Color::TRANSPARENT` (the default) falls back to the theme's washes and focus ring.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub color: Reactive<Color>,
}

/// A long list of rows to choose one from, with only the rows on screen built.
pub fn virtual_list(
    props: VirtualListProps,
    _children: Children,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let VirtualListProps {
        items,
        selected,
        on_select,
        row,
        label,
        color,
    } = props;
    let selected = selected.unwrap_or_else(|| signal(None));
    let rows = memo(move || Rows(Rc::new(items.get())));
    let row_height = ListTokens::current().row_height();

    let mut made: Option<Rc<List>> = None;
    let scroll = LayoutScrollArea::new_with(LayoutStyle::new().flex_grow(1.0), |viewport| {
        let list = Rc::new(List {
            rows,
            cursor: signal(None),
            selected,
            search: RefCell::default(),
            on_select,
            viewport,
            row_height,
            focus: Cell::new(None),
            built: BuiltRows::new(),
        });
        made = Some(list.clone());
        let source = {
            let list = list.clone();
            move || placed(&list.rows())
        };
        let built = VirtualList::new(
            LayoutStyle::column(),
            list.viewport.clone(),
            row_height,
            OVERSCAN,
            source,
            |item: &(ListRow, SetPosition)| item.clone(),
            move |_, (item, place)| build_row(&list, item, place, row.as_ref(), &color),
        )?;
        Ok(box_item(built))
    })?;
    let list = made.expect("a scroll area builds its content before it returns");

    let (seeding, keys, cursor) = (list.clone(), list.clone(), list.clone());
    let scroll = scroll
        .control(Role::ListBox)
        .focus_style(|_r| RectStyle::default())
        .on_focus(move |now| {
            if now {
                seeding.seed_cursor();
            }
        })
        .active_descendant(move || cursor.cursor_row())
        .on_focused_key(move |key: &Key| keys.on_key(key));
    list.focus.set(scroll.focus_id());
    Ok(match label {
        Some(label) => box_item(scroll.a11y_label(move || label.get())),
        None => box_item(scroll),
    })
}

/// The rows as given. Equal only to itself, so the memo notifies on every new list rather than comparing whole lists.
#[derive(Clone)]
struct Rows(Rc<Vec<ListRow>>);

impl PartialEq for Rows {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

struct List {
    rows: Memo<Rows>,
    /// The row the keyboard is on. Deliberately not the selection: walking the list chooses nothing.
    cursor: RwSignal<Option<Arc<str>>>,
    selected: RwSignal<Option<Arc<str>>>,
    search: RefCell<TypeAhead>,
    on_select: Option<RowHandler>,
    viewport: ScrollViewport,
    row_height: f32,
    focus: Cell<Option<FocusId>>,
    built: BuiltRows<Arc<str>>,
}

impl List {
    fn rows(&self) -> Rc<Vec<ListRow>> {
        self.rows.with(|rows| rows.0.clone())
    }

    fn index_of(rows: &[ListRow], id: Option<Arc<str>>) -> Option<usize> {
        let id = id?;
        rows.iter().position(|row| row.id == id)
    }

    /// Where the keys act from: the cursor while its row is listed, else the selection's row.
    fn at(&self, rows: &[ListRow]) -> Option<usize> {
        Self::index_of(rows, self.cursor.peek())
            .or_else(|| Self::index_of(rows, self.selected.peek()))
    }

    fn is_selected(&self, id: &str) -> bool {
        self.selected
            .with(|selected| selected.as_deref() == Some(id))
    }

    fn shows_cursor(&self, id: &str) -> bool {
        self.focus.get().is_some_and(focus::is_focus_visible)
            && self.cursor.with(|cursor| cursor.as_deref() == Some(id))
    }

    fn cursor_row(&self) -> Option<FocusId> {
        self.built.get(&self.cursor.get()?)
    }

    /// Puts the cursor where Tab arriving at the list should find it: on the selection, or on the first row it can stop on.
    fn seed_cursor(&self) {
        let rows = self.rows();
        if Self::index_of(&rows, self.cursor.peek()).is_some() {
            return;
        }
        let start = Self::index_of(&rows, self.selected.peek())
            .or_else(|| walk(&rows, None, KeyNavMove::First));
        if let Some(i) = start {
            self.cursor.set(Some(rows[i].id.clone()));
        }
    }

    fn move_to(&self, rows: &[ListRow], index: usize) {
        self.cursor.set(Some(rows[index].id.clone()));
        reveal_band(
            &self.viewport,
            index as f32 * self.row_height,
            self.row_height,
        );
    }

    fn choose(&self, row: &ListRow) {
        if row.disabled {
            return;
        }
        self.cursor.set(Some(row.id.clone()));
        self.selected.set(Some(row.id.clone()));
        if let Some(on_select) = &self.on_select {
            on_select(row);
        }
    }

    fn choose_id(&self, id: &str) {
        let rows = self.rows();
        if let Some(row) = rows.iter().find(|row| &*row.id == id) {
            self.choose(row);
        }
    }

    fn take_focus(&self) {
        if let Some(id) = self.focus.get() {
            focus::request_from_pointer(id);
        }
    }

    /// How many rows one Page Up or Page Down moves: as many as the view shows whole.
    fn page(&self) -> usize {
        let height = self.viewport.rect().peek().height;
        ((height / self.row_height).floor() as usize).max(1)
    }

    fn seek(&self, c: char, rows: &[ListRow], at: Option<usize>) {
        let needle = self.search.borrow_mut().extend(c);
        let found = needle.find(at, rows.len(), |i| {
            (!rows[i].disabled).then(|| rows[i].label.to_string())
        });
        if let Some(i) = found {
            self.move_to(rows, i);
        }
    }

    fn on_key(&self, key: &Key) -> bool {
        let rows = self.rows();
        if rows.is_empty() {
            return false;
        }
        let at = self.at(&rows);
        let modifiers = telar::modifiers();
        let paged = |forward: bool| {
            let from = at.unwrap_or(0);
            let to = match forward {
                true => (from + self.page()).min(rows.len() - 1),
                false => from.saturating_sub(self.page()),
            };
            nearest_reachable(&rows, to, forward)
        };
        let target = match key {
            Key::Named(NamedKey::Space) if self.search.borrow().is_searching() => {
                self.seek(' ', &rows, at);
                return true;
            }
            Key::Named(NamedKey::Enter | NamedKey::Space) => {
                if let Some(i) = at {
                    self.choose(&rows[i]);
                }
                return true;
            }
            Key::Named(NamedKey::PageDown) => paged(true),
            Key::Named(NamedKey::PageUp) => paged(false),
            Key::Char(c) if !c.is_control() && !modifiers.is_ctrl && !modifiers.is_meta => {
                self.seek(*c, &rows, at);
                return true;
            }
            key => match KeyNav::default().interpret(key) {
                Some(KeyNavMove::Cancel | KeyNavMove::Activate) | None => return false,
                Some(movement) => walk(&rows, at, movement),
            },
        };
        if let Some(i) = target {
            self.move_to(&rows, i);
        }
        true
    }
}

/// Each row with its place among them all, which a reader cannot count from the rows that are built.
fn placed(rows: &[ListRow]) -> Vec<(ListRow, SetPosition)> {
    let size = rows.len() as u32;
    rows.iter()
        .enumerate()
        .map(|(i, row)| {
            let place = SetPosition {
                level: 1,
                position: i as u32 + 1,
                size,
            };
            (row.clone(), place)
        })
        .collect()
}

/// Where `movement` takes the cursor from `at`, passing over rows it may not stop on; `None` when there are none it may.
fn walk(rows: &[ListRow], at: Option<usize>, movement: KeyNavMove) -> Option<usize> {
    let count = rows.len();
    let reachable = |i: usize| !rows[i].disabled;
    match (movement, at) {
        (KeyNavMove::Last, _) => nearest_reachable(rows, count.checked_sub(1)?, false),
        (KeyNavMove::First, _) | (_, None) => nearest_reachable(rows, 0, true),
        (movement, Some(from)) => {
            let mut i = from;
            for _ in 0..count {
                i = key_nav_apply(i, count, movement);
                if reachable(i) {
                    return Some(i);
                }
            }
            None
        }
    }
}

/// The first row from `from` the cursor may stop on, looking `forward` or back, and the other way when there is none that way.
fn nearest_reachable(rows: &[ListRow], from: usize, forward: bool) -> Option<usize> {
    let ahead = |forward: bool| -> Option<usize> {
        match forward {
            true => (from..rows.len()).find(|&i| !rows[i].disabled),
            false => (0..=from).rev().find(|&i| !rows[i].disabled),
        }
    };
    ahead(forward).or_else(|| ahead(!forward))
}

fn build_row(
    list: &Rc<List>,
    row: ListRow,
    place: SetPosition,
    content: Option<&RowBuilder>,
    color: &Reactive<Color>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let content = match content {
        Some(build) => vec![build(&row)?],
        None => default_content(&row)?,
    };
    let id = row.id.clone();
    let disabled = row.disabled;
    let paint = |hovered: bool| {
        let (list, id, color) = (list.clone(), id.clone(), color.clone());
        move |_r| {
            cursor_row_paint(
                ListTokens::current().radius(),
                &color,
                RowState {
                    selected: list.is_selected(&id),
                    hovered: hovered && !disabled,
                    cursor: list.shows_cursor(&id),
                },
            )
        }
    };
    let selected = {
        let (list, id) = (list.clone(), id.clone());
        move || list.is_selected(&id)
    };
    let press = {
        let (list, id) = (list.clone(), id.clone());
        move || {
            list.choose_id(&id);
            list.take_focus();
        }
    };
    let height = list.row_height;
    let row_box = move || {
        LayoutStyle::row()
            .align_items(AlignItems::CENTER)
            .height(height)
            .flex_shrink(0.0)
            .padding_horizontal(ListTokens::current().pad_x())
            .gap(shared::gap_md())
    };
    let built = StyledContainer::new(row_box(), paint(false), content)?
        .styled_by(row_box)
        .hover_style(paint(true))
        .disabled(move || disabled)
        .presented(Role::Option)
        .positioned(place)
        .toggled(selected)
        .on_press(press);
    if let Some(focus_id) = built.focus_id() {
        list.built.track(id, focus_id);
    }
    Ok(box_item(built))
}

/// A row's label, taking the room the row has, and its detail after it, quiet.
fn default_content(row: &ListRow) -> Result<Vec<Box<dyn LayoutItem>>, LayoutError> {
    let label = row.label.clone();
    let disabled = row.disabled;
    let text = Text::declaring(
        move || label.to_string(),
        LayoutStyle::new()
            .flex_grow(1.0)
            .flex_shrink(1.0)
            .min_width(0.0),
        move |t| {
            let text = shared::control_text(t, 1.0).with_text_wrap(TextWrap::NoWrap);
            if disabled {
                text.with_color(shared::muted())
            } else {
                text
            }
        },
    )?;
    let mut content: Vec<Box<dyn LayoutItem>> =
        vec![box_item(ClippedItem::new(box_item(text), Clip::both()))];
    if let Some(detail) = row.detail.clone() {
        content.push(box_item(Text::declaring(
            move || detail.to_string(),
            LayoutStyle::new().flex_shrink(0.0),
            |t| shared::quiet(t, shared::CAPTION_RATIO).with_text_wrap(TextWrap::NoWrap),
        )?));
    }
    Ok(content)
}

#[cfg(test)]
#[path = "virtual_list_test.rs"]
mod tests;
