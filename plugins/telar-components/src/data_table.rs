//! [`data_table`]: rows of data under a header of columns — sorted by a column, resized from its header's edge, walked cell by cell from the keyboard and chosen one or several at a time, with only the rows on screen built.

use std::cell::{Cell, RefCell};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use std::sync::Arc;

use telar::{
    Accessible, AlignItems, Border, Children, Clip, ClippedItem, Color, Container, JustifyContent,
    Key, KeyNav, KeyNavMove, LayoutError, LayoutItem, LayoutScrollArea, LayoutStyle, Memo,
    NamedKey, Props, Reactive, Rect, RectStyle, RwSignal, ScrollbarStyle, ShapeStyle, Size,
    SortDirection, StyledContainer, Text, TextAlign, TextWrap, box_item, children,
    current_direction,
    focus::{self, FocusId, Role, SetPosition},
    key_nav_apply_grid, memo, signal, track_layout,
};

use crate::resizable::{ResizeEdge, ResizeHandleProps, resize_handle};
use crate::rows::{BuiltRows, RowPlaces, RowState, counted, cursor_row_paint, with_empty_state};
use crate::selection::{Gesture, RowSelection, SelectionHandler, SelectionMode};
use crate::{shared, strings};

const OVERSCAN: usize = 4;
const MIN_COLUMN_WIDTH: f32 = 48.0;

/// A data table's own tokens, which an application's theme supplies through a `#[theme(extension)]` field.
///
/// Every field left `None` follows the role named beside it, read when the table is built or painted, so a table's rows stand at the height every other list's do.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DataTableTokens {
    /// Height of a row, the header's included, at the regular control size and scaled with the control size in force. `None` is [`metrics::row_height`](crate::metrics::row_height).
    pub row_height: Option<f32>,
    /// Space between a cell's edges and what it shows, likewise. `None` is [`metrics::gap_md`](crate::metrics::gap_md).
    pub pad_x: Option<f32>,
    /// Corner of a row's wash and of the keyboard's cursor. `None` is the theme's `radius_item`.
    pub radius: Option<f32>,
}

impl DataTableTokens {
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

    pub fn pad_x(&self) -> f32 {
        self.pad_x
            .map_or_else(shared::gap_md, |pad| pad * telar::control_scale())
    }

    pub fn radius(&self) -> f32 {
        self.radius.unwrap_or_else(shared::radius_item)
    }
}

/// One row of a [`data_table`], as data: the id the selection names it by, and the text of each of its cells in the order of the table's columns.
///
/// The text is what a cell shows and what its column sorts by, unless the column builds its cells or compares its rows itself.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TableRow {
    /// Unique among the table's rows.
    pub id: Arc<str>,
    pub cells: Arc<[Arc<str>]>,
}

impl TableRow {
    pub fn new<C: Into<Arc<str>>>(
        id: impl Into<Arc<str>>,
        cells: impl IntoIterator<Item = C>,
    ) -> Self {
        Self {
            id: id.into(),
            cells: cells.into_iter().map(Into::into).collect(),
        }
    }

    /// The text of the cell in column `index`, empty where the row has none.
    pub fn cell(&self, index: usize) -> &str {
        self.cells.get(index).map_or("", |cell| cell)
    }
}

/// How wide a column is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ColumnWidth {
    /// This many logical px, whatever the table's width; columns that add up to more than the table scroll it sideways.
    Fixed(f32),
    /// A share of the width the fixed columns leave, against the other flexible columns' shares.
    Flex(f32),
}

impl Default for ColumnWidth {
    fn default() -> Self {
        Self::Flex(1.0)
    }
}

/// The widths the user gave the columns resized from their headers, in logical px by column id: what [`DataTableProps::widths`] binds, and what an application keeps to give the table its layout back.
pub type ColumnWidths = BTreeMap<Arc<str>, f32>;

/// Where a cell's content sits across its column.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CellAlign {
    /// At the start of the line: the left, or the right in a right-to-left region.
    #[default]
    Start,
    Center,
    /// At the end of the line, where a column of figures lines up.
    End,
}

/// What [`Column::cell`] builds a cell's content with.
pub type CellBuilder = Rc<dyn Fn(&TableRow) -> Result<Box<dyn LayoutItem>, LayoutError>>;

/// What [`Column::compare`] orders two rows with, ascending.
pub type Comparator = Rc<dyn Fn(&TableRow, &TableRow) -> Ordering>;

/// One column of a [`data_table`]: its header, its width, where its cells sit, and whether and how it sorts the rows and is resized.
#[derive(Clone)]
pub struct Column {
    /// Unique among the table's columns: what a [`TableSort`] and the [`ColumnWidths`] name it by.
    pub id: Arc<str>,
    /// The header's text.
    pub label: Arc<str>,
    /// The width it starts at, and keeps until it is resized.
    pub width: ColumnWidth,
    pub align: CellAlign,
    /// Whether a press or Enter on its header sorts the rows by it.
    pub sortable: bool,
    /// Whether its header's inline-end edge resizes it.
    pub resizable: bool,
    /// The narrowest a resize leaves it, in logical px. 48 unless set.
    pub min_width: f32,
    /// Whether its cells may run onto more lines than one: its text wraps at the column's width instead of being cut off where the cell ends, and a cell it [builds](Self::cell) is given the height it asks for, the row growing to hold its tallest cell. A table with such a column measures each row as it is built; one without stands every row at [`DataTableTokens::row_height`].
    pub wrap: bool,
    /// Builds each cell's content in place of the row's text for this column.
    pub cell: Option<CellBuilder>,
    /// Orders two rows by this column, ascending, in place of comparing their text: numbers as numbers, anything else as words.
    pub compare: Option<Comparator>,
}

impl Column {
    pub fn new(id: impl Into<Arc<str>>, label: impl Into<Arc<str>>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            width: ColumnWidth::default(),
            align: CellAlign::default(),
            sortable: false,
            resizable: false,
            min_width: MIN_COLUMN_WIDTH,
            wrap: false,
            cell: None,
            compare: None,
        }
    }

    pub fn fixed(mut self, width: f32) -> Self {
        self.width = ColumnWidth::Fixed(width);
        self
    }

    pub fn flex(mut self, share: f32) -> Self {
        self.width = ColumnWidth::Flex(share);
        self
    }

    pub fn align(mut self, align: CellAlign) -> Self {
        self.align = align;
        self
    }

    pub fn sortable(mut self, sortable: bool) -> Self {
        self.sortable = sortable;
        self
    }

    pub fn resizable(mut self, resizable: bool) -> Self {
        self.resizable = resizable;
        self
    }

    pub fn min_width(mut self, width: f32) -> Self {
        self.min_width = width;
        self
    }

    pub fn wrap(mut self, wrap: bool) -> Self {
        self.wrap = wrap;
        self
    }

    pub fn cell(
        mut self,
        build: impl Fn(&TableRow) -> Result<Box<dyn LayoutItem>, LayoutError> + 'static,
    ) -> Self {
        self.cell = Some(Rc::new(build));
        self
    }

    pub fn compare(mut self, compare: impl Fn(&TableRow, &TableRow) -> Ordering + 'static) -> Self {
        self.compare = Some(Rc::new(compare));
        self
    }
}

/// Which column the rows are sorted by, and which way.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TableSort {
    pub column: Arc<str>,
    pub direction: SortDirection,
}

/// What [`DataTableProps::on_sort`] is called with.
pub type SortHandler = Rc<dyn Fn(&TableSort)>;

/// What [`DataTableProps::on_select`] is called with.
pub type TableRowHandler = Rc<dyn Fn(&TableRow)>;

/// Rows of data under a header of columns, read by a screen reader as a grid.
///
/// It fills the height its parent gives it and scrolls within it, header held at the top, building only the rows on screen, so a table of thousands of rows costs what a screenful does. Every row stands at [`DataTableTokens::row_height`], unless a column [wraps](Column::wrap): then each row is measured once built, as tall as its tallest cell, and stands at the row height until then.
///
/// The keyboard walks a cursor from cell to cell — the arrows, Home and End along a row, Ctrl with them to the table's corners, Page Up and Page Down — and a reader hears the cell it is on. Enter, Space or a press on a row's cell chooses the row; on a sortable column's header they sort the rows by it, and again the other way. In [`SelectionMode::Multiple`] the grid says it takes several rows, each row says whether it is one of them, and Shift, Ctrl (Command on a Mac) and Space choose them as [`SelectionMode::Multiple`] says.
///
/// A [resizable](Column::resizable) column's header holds a splitter on its inline-end edge, the [`resize_handle`] a reader hears as "Resize Name column" with the column's width as its value: dragged, or focused with Tab after the table and stepped with the arrow keys, Home taking it to its [narrowest](Column::min_width), and Escape putting a drag back. A column resized keeps the width it was given in [`widths`](Self::widths) as though it were [`Fixed`](ColumnWidth::Fixed) — a [`Flex`](ColumnWidth::Flex) one too, so the flexible columns left share what the fixed ones leave, and columns that add up to more than the table scroll it sideways.
///
/// Children: what the table shows under its header while it has no rows, in place of [`empty_label`](Self::empty_label).
#[derive(Props)]
pub struct DataTableProps {
    #[props(into, default)]
    pub columns: Vec<Column>,
    #[props(into, default)]
    pub rows: Reactive<Vec<TableRow>>,
    /// The bound sort. `None` (the default) is uncontrolled and starts unsorted.
    #[props(some, into, default)]
    pub sort: Option<RwSignal<Option<TableSort>>>,
    /// Fired with the sort a header asked for.
    #[props(some, default)]
    pub on_sort: Option<SortHandler>,
    /// Whether the table orders its rows by the sort itself. On by default; off for rows the application orders as it hears [`on_sort`](Self::on_sort) — rows fetched sorted — which the table then shows as given.
    #[props(default = true)]
    pub sort_rows: bool,
    /// The bound widths of the columns resized. `None` (the default) is uncontrolled and starts with every column at its own [`width`](Column::width).
    #[props(some, into, default)]
    pub widths: Option<RwSignal<ColumnWidths>>,
    /// How many rows can be chosen at once.
    #[props(default)]
    pub selection_mode: SelectionMode,
    /// The bound selection in [`SelectionMode::Single`], by row id. `None` (the default) is uncontrolled.
    #[props(some, into, default)]
    pub selected: Option<RwSignal<Option<Arc<str>>>>,
    /// The bound selection in [`SelectionMode::Multiple`], by row id. `None` (the default) is uncontrolled and starts empty.
    #[props(some, into, default)]
    pub selected_rows: Option<RwSignal<BTreeSet<Arc<str>>>>,
    /// Fired with the row a press or Enter chose on its own, and in [`SelectionMode::Single`] the row Space chose.
    #[props(some, default)]
    pub on_select: Option<TableRowHandler>,
    /// Fired with every row chosen each time a press or a key changes them, in [`SelectionMode::Multiple`].
    #[props(some, default)]
    pub on_selection_change: Option<SelectionHandler>,
    /// What a reader calls the table.
    #[props(some, into, default)]
    pub label: Option<Reactive<String>>,
    /// What the table says under its header while it has no rows, and a reader hears as a status. `None` (the default) uses the localized `telar_components.nothing_to_show` message.
    #[props(some, into, default)]
    pub empty_label: Option<Reactive<String>>,
    /// Accent for the selection and the keyboard cursor. `Color::TRANSPARENT` (the default) falls back to the theme's washes and focus ring.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub color: Reactive<Color>,
}

/// Rows of data under a header of sortable, resizable columns, walked cell by cell.
pub fn data_table(
    props: DataTableProps,
    children: Children,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let DataTableProps {
        columns,
        rows,
        sort,
        on_sort,
        sort_rows,
        widths,
        selection_mode,
        selected,
        selected_rows,
        on_select,
        on_selection_change,
        label,
        empty_label,
        color,
    } = props;
    let columns: Rc<[Column]> = columns.into();
    let sort = sort.unwrap_or_else(|| signal(None));
    let widths = widths.unwrap_or_else(|| signal(ColumnWidths::new()));
    let selection = RowSelection::new(selection_mode, selected, selected_rows, on_selection_change);
    let ordered = {
        let columns = columns.clone();
        memo(move || {
            let mut rows = rows.get();
            if sort_rows {
                order(&mut rows, &columns, sort.get().as_ref());
            }
            Ordered(Rc::new(rows))
        })
    };
    let row_height = DataTableTokens::current().row_height();
    let wraps = columns.iter().any(|column| column.wrap);

    let mut made: Option<Rc<Table>> = None;
    let scroll = LayoutScrollArea::new_with(LayoutStyle::new().flex_grow(1.0), |viewport| {
        let table = Rc::new(Table {
            columns,
            rows: ordered,
            sort,
            on_sort,
            widths,
            selection,
            on_select,
            cursor: signal(None),
            places: RowPlaces::new(viewport, row_height, wraps).under_header(row_height),
            row_height,
            focus: Cell::new(None),
            built: BuiltRows::new(),
            color,
        });
        made = Some(table.clone());
        let header = header_row(&table)?;
        let source = {
            let table = table.clone();
            move || counted(&table.rows())
        };
        let body = {
            let table = table.clone();
            table.places.clone().list(
                OVERSCAN,
                source,
                |item: &(TableRow, usize)| item.clone(),
                move |index, (row, count)| body_row(&table, row, line_place(index + 1, count)),
            )?
        };
        let content = Container::new(LayoutStyle::column(), children![header, body])?;
        Ok(box_item(content))
    })?;
    let table = made.expect("a scroll area builds its content before it returns");

    let (seeding, keys, cursor) = (table.clone(), table.clone(), table.clone());
    let scroll = scroll
        .control(Role::Grid)
        .focus_style(|_r| RectStyle::default())
        .on_focus(move |now| {
            if now {
                seeding.seed_cursor();
            }
        })
        .active_descendant(move || cursor.cursor_cell())
        .on_focused_key(move |key: &Key| keys.on_key(key));
    let scroll = match table.selection.is_multiple() {
        true => scroll.multiselectable(),
        false => scroll,
    };
    table.focus.set(scroll.focus_id());
    let scroll = match label {
        Some(label) => box_item(scroll.a11y_label(move || label.get())),
        None => box_item(scroll),
    };
    with_empty_state(
        scroll,
        move || ordered.with(|rows| rows.0.is_empty()),
        row_height,
        empty_label,
        children,
    )
}

/// The row on `line` of a grid of `rows` rows under its header, as the grid counts its lines: the header first.
fn line_place(line: usize, rows: usize) -> SetPosition {
    SetPosition {
        level: 1,
        position: line as u32 + 1,
        size: rows as u32 + 1,
    }
}

/// The ids of the rows in the order shown: what a range and Ctrl+A choose among.
fn ids(rows: &[TableRow]) -> Vec<Arc<str>> {
    rows.iter().map(|row| row.id.clone()).collect()
}

/// Sorts `rows` by `sort`'s column, stably, the column's own comparison first and its text's after.
fn order(rows: &mut [TableRow], columns: &[Column], sort: Option<&TableSort>) {
    let Some(sort) = sort else {
        return;
    };
    let Some(index) = columns.iter().position(|column| column.id == sort.column) else {
        return;
    };
    let column = &columns[index];
    rows.sort_by(|a, b| {
        let ascending = match &column.compare {
            Some(compare) => compare(a, b),
            None => compare_text(a.cell(index), b.cell(index)),
        };
        match sort.direction {
            SortDirection::Ascending => ascending,
            SortDirection::Descending => ascending.reverse(),
        }
    });
}

/// Two cells' text in the order a reader expects: as numbers when both are numbers, else as words, case aside.
fn compare_text(a: &str, b: &str) -> Ordering {
    match (a.trim().parse::<f64>(), b.trim().parse::<f64>()) {
        (Ok(a), Ok(b)) => a.total_cmp(&b),
        _ => a
            .to_lowercase()
            .cmp(&b.to_lowercase())
            .then_with(|| a.cmp(b)),
    }
}

/// The rows in the order shown. Equal only to itself, so the memo notifies on every reorder rather than comparing whole tables.
#[derive(Clone)]
struct Ordered(Rc<Vec<TableRow>>);

impl PartialEq for Ordered {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// Where the keyboard's cursor is: a row by id, `None` for the header, and a column by index.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct At {
    row: Option<Arc<str>>,
    column: usize,
}

struct Table {
    columns: Rc<[Column]>,
    rows: Memo<Ordered>,
    sort: RwSignal<Option<TableSort>>,
    on_sort: Option<SortHandler>,
    widths: RwSignal<ColumnWidths>,
    selection: RowSelection,
    on_select: Option<TableRowHandler>,
    /// Deliberately not the selection: walking the table chooses nothing.
    cursor: RwSignal<Option<At>>,
    places: RowPlaces,
    /// The header's height, and the least a row stands at.
    row_height: f32,
    focus: Cell<Option<FocusId>>,
    built: BuiltRows<At>,
    color: Reactive<Color>,
}

impl Table {
    fn rows(&self) -> Rc<Vec<TableRow>> {
        self.rows.with(|rows| rows.0.clone())
    }

    fn width(&self) -> usize {
        self.columns.len()
    }

    /// How wide the column at `index` stands now: at the width it was resized to, never under its narrowest, else at its own.
    fn width_of(&self, index: usize) -> ColumnWidth {
        let column = &self.columns[index];
        match self.widths.with(|widths| widths.get(&column.id).copied()) {
            Some(width) => ColumnWidth::Fixed(width.max(column.min_width)),
            None => column.width,
        }
    }

    /// The cursor as a line of the grid — `0` the header, `1 + i` the row at `i` — and a column; `None` while it names a row no longer shown.
    fn line_of(&self, rows: &[TableRow], at: &At) -> Option<(usize, usize)> {
        let line = match &at.row {
            None => 0,
            Some(id) => 1 + rows.iter().position(|row| &row.id == id)?,
        };
        Some((line, at.column.min(self.width().saturating_sub(1))))
    }

    fn at_line(rows: &[TableRow], line: usize, column: usize) -> At {
        At {
            row: line.checked_sub(1).map(|i| rows[i].id.clone()),
            column,
        }
    }

    fn shows_cursor(&self, at: &At) -> bool {
        self.focus.get().is_some_and(focus::is_focus_visible)
            && self.cursor.with(|cursor| cursor.as_ref() == Some(at))
    }

    fn cursor_cell(&self) -> Option<FocusId> {
        self.built.get(&self.cursor.get()?)
    }

    fn sorted_by(&self, column: &str) -> Option<SortDirection> {
        self.sort.with(|sort| {
            sort.as_ref()
                .filter(|sort| &*sort.column == column)
                .map(|sort| sort.direction)
        })
    }

    /// Puts the cursor where Tab arriving at the table should find it: on the first row chosen, else on the first row, else on the header.
    fn seed_cursor(&self) {
        let rows = self.rows();
        if self
            .cursor
            .peek()
            .is_some_and(|at| self.line_of(&rows, &at).is_some())
        {
            return;
        }
        let row = self
            .selection
            .first_of(rows.iter().map(|row| &row.id))
            .or_else(|| rows.first().map(|row| row.id.clone()));
        if let Some(row) = &row {
            self.selection.anchor_at(row);
        }
        self.cursor.set(Some(At { row, column: 0 }));
    }

    fn move_to(&self, rows: &[TableRow], line: usize, column: usize) {
        let at = Self::at_line(rows, line, column);
        self.cursor.set(Some(at.clone()));
        self.reveal(line, &at);
    }

    /// Moves the cursor from the row on line `from` to `line`, choosing every row between the anchor and it when `ranging`, and making its row the anchor otherwise.
    fn walk_to(
        &self,
        rows: &[TableRow],
        (line, column): (usize, usize),
        from: usize,
        ranging: bool,
    ) {
        self.move_to(rows, line, column);
        let Some(index) = line.checked_sub(1) else {
            return;
        };
        let id = &rows[index].id;
        if ranging {
            let from = from.checked_sub(1).map(|i| rows[i].id.clone());
            self.selection.apply(
                Gesture::Range { keep: false },
                id,
                || ids(rows),
                from.as_ref(),
            );
        } else {
            self.selection.anchor_at(id);
        }
    }

    /// Scrolls the least distance that brings the cursor's cell into view, below the header that stays over the rows: down by the row's place, which is known whether or not it is built, and across by the cell's, once it is.
    fn reveal(&self, line: usize, at: &At) {
        if let Some(index) = line.checked_sub(1) {
            self.places.reveal(index);
        }
        let viewport = self.places.viewport();
        if viewport.rect().peek().height <= 0.0 {
            return;
        }
        let Some(cell) = self
            .built
            .get(at)
            .and_then(focus::node_of)
            .and_then(track_layout)
        else {
            return;
        };
        let cell = cell.peek();
        let (_, y) = viewport.peek_offset();
        viewport.reveal_rect(Rect::new(cell.x, y, cell.width, 0.0), 0.0);
    }

    /// The line Page Down (`forward`) or Page Up takes the cursor to from `line`: on by as many rows as the view shows whole under the header.
    fn paged(&self, line: usize, rows: usize, forward: bool) -> usize {
        self.places
            .page(line.checked_sub(1), rows, forward)
            .map_or(0, |index| index + 1)
    }

    /// Does what `gesture` asks of the row at `index`, and tells [`on_select`](DataTableProps::on_select) of a row chosen on its own.
    fn act(&self, rows: &[TableRow], index: usize, gesture: Gesture) {
        let row = &rows[index];
        let from = self.cursor.peek().and_then(|at| at.row);
        self.selection
            .apply(gesture, &row.id, || ids(rows), from.as_ref());
        if gesture == Gesture::Only
            && let Some(on_select) = &self.on_select
        {
            on_select(row);
        }
    }

    /// Sorts the rows by `column`, ascending the first time and the other way each time after.
    fn sort_by(&self, column: usize) {
        let Some(column) = self.columns.get(column).filter(|column| column.sortable) else {
            return;
        };
        let direction = self
            .sorted_by(&column.id)
            .map_or(SortDirection::Ascending, SortDirection::reversed);
        let sort = TableSort {
            column: column.id.clone(),
            direction,
        };
        self.sort.set(Some(sort.clone()));
        if let Some(on_sort) = &self.on_sort {
            on_sort(&sort);
        }
    }

    /// What a press on the cell `at` does: sorts by its column on the header, and anywhere else chooses its row as the modifiers held say.
    fn press(&self, at: &At) {
        let rows = self.rows();
        if let Some((line, column)) = self.line_of(&rows, at) {
            match line.checked_sub(1) {
                None => self.sort_by(column),
                Some(index) => {
                    let gesture = Gesture::of_press(self.selection.mode(), telar::modifiers());
                    self.act(&rows, index, gesture);
                }
            }
            self.cursor.set(Some(at.clone()));
        }
        if let Some(id) = self.focus.get() {
            focus::request_from_pointer(id);
        }
    }

    fn on_key(&self, key: &Key) -> bool {
        let rows = self.rows();
        let width = self.width();
        if width == 0 {
            return false;
        }
        let lines = rows.len() + 1;
        let (line, column) = self
            .cursor
            .peek()
            .and_then(|at| self.line_of(&rows, &at))
            .unwrap_or((usize::from(!rows.is_empty()), 0));
        let modifiers = telar::modifiers();
        let multiple = self.selection.is_multiple();
        let corner = modifiers.is_ctrl || modifiers.is_meta;
        let to = match key {
            Key::Char('a' | 'A') if multiple && modifiers.is_primary() => {
                self.selection.all(&ids(&rows));
                return true;
            }
            Key::Named(NamedKey::Enter | NamedKey::Space) if line == 0 => {
                self.sort_by(column);
                return true;
            }
            Key::Named(NamedKey::Space) if multiple => {
                let gesture = match modifiers.is_shift {
                    true => Gesture::Range { keep: false },
                    false => Gesture::Toggle,
                };
                self.act(&rows, line - 1, gesture);
                return true;
            }
            Key::Named(NamedKey::Enter | NamedKey::Space) => {
                self.act(&rows, line - 1, Gesture::Only);
                return true;
            }
            Key::Named(NamedKey::Home) if corner => (0, 0),
            Key::Named(NamedKey::End) if corner => (lines - 1, width - 1),
            Key::Named(NamedKey::Home) => (line, 0),
            Key::Named(NamedKey::End) => (line, width - 1),
            Key::Named(NamedKey::PageDown) => (self.paged(line, rows.len(), true), column),
            Key::Named(NamedKey::PageUp) => (self.paged(line, rows.len(), false), column),
            key => match KeyNav::default()
                .grid()
                .reading(current_direction())
                .interpret(key)
            {
                Some(
                    movement @ (KeyNavMove::Next
                    | KeyNavMove::Previous
                    | KeyNavMove::NextRow
                    | KeyNavMove::PreviousRow),
                ) => {
                    let cell =
                        key_nav_apply_grid(line * width + column, lines * width, width, movement);
                    (cell / width, cell % width)
                }
                _ => return false,
            },
        };
        self.walk_to(&rows, to, line, multiple && modifiers.is_shift);
        true
    }
}

/// A cell's box in the column at `index`, in the header or in a row: its width, its height, where its content sits across it, and the space from its edges.
fn cell_box(table: &Table, index: usize, header: bool) -> LayoutStyle {
    let style = LayoutStyle::row()
        .align_items(AlignItems::CENTER)
        .justify_content(match table.columns[index].align {
            CellAlign::Start => JustifyContent::START,
            CellAlign::Center => JustifyContent::CENTER,
            CellAlign::End => JustifyContent::END,
        })
        .min_width(0.0)
        .padding_horizontal(DataTableTokens::current().pad_x());
    let style = match header {
        true => style.height(table.row_height),
        false => table.places.sized(style),
    };
    match table.width_of(index) {
        ColumnWidth::Fixed(width) => style.width(width).flex_shrink(0.0),
        ColumnWidth::Flex(share) => style.flex_grow(share).flex_shrink(1.0).flex_basis(0.0),
    }
}

/// A row's box, the header's included: `height` tall, or for `None` as tall as its tallest cell with every cell stretched to it. Its cells stop short of the lane the scroll area draws its vertical bar in, on the right in either direction, so the bar never covers the last column or takes the press meant for its splitter, and the row's wash still runs under it.
fn line_box(height: Option<f32>) -> LayoutStyle {
    let style = LayoutStyle::row()
        .flex_shrink(0.0)
        .padding_right(ScrollbarStyle::default().width);
    match height {
        Some(height) => style.align_items(AlignItems::CENTER).height(height),
        None => style.align_items(AlignItems::STRETCH),
    }
}

/// `text` on one line, cut off where its cell ends.
fn cell_text(
    text: Arc<str>,
    style: impl Fn(telar::TextStyle) -> telar::TextStyle + 'static,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let text = Text::declaring(
        move || text.to_string(),
        LayoutStyle::new().flex_shrink(1.0).min_width(0.0),
        move |t| style(t).with_text_wrap(TextWrap::NoWrap),
    )?;
    Ok(box_item(ClippedItem::new(box_item(text), Clip::both())))
}

/// `text` on as many lines as its cell's width needs, each set across it as `align` says.
fn wrapped_text(text: Arc<str>, align: CellAlign) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let align = match align {
        CellAlign::Start => TextAlign::Start,
        CellAlign::Center => TextAlign::Center,
        CellAlign::End => TextAlign::End,
    };
    let text = Text::declaring(
        move || text.to_string(),
        LayoutStyle::new()
            .flex_grow(1.0)
            .flex_shrink(1.0)
            .min_width(0.0),
        move |t| {
            shared::control_text(t, 1.0)
                .with_text_wrap(TextWrap::Wrap)
                .with_text_align(align)
        },
    )?;
    Ok(box_item(text))
}

/// The header: one cell naming each column, held at the top of the view while the rows scroll under it, on an opaque surface they cannot show through.
fn header_row(table: &Rc<Table>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let cells = (0..table.width())
        .map(|index| header_cell(table, index))
        .collect::<Result<Vec<_>, _>>()?;
    let height = table.row_height;
    let row_box = move || line_box(Some(height)).sticky().inset_top(0.0);
    let header = StyledContainer::new(
        row_box(),
        |_r| {
            RectStyle::default()
                .with_fill(shared::surface())
                .with_border(Border::per_side(
                    shared::border(),
                    0.0,
                    0.0,
                    shared::border_width(),
                    0.0,
                ))
        },
        cells,
    )?
    .styled_by(row_box)
    .role(Role::Row)
    .positioned_by({
        let table = table.clone();
        move || line_place(0, table.rows().len())
    });
    Ok(box_item(header))
}

/// A column's header, named by the column's label alone so a reader does not hear its splitter's name in it too.
fn header_cell(table: &Rc<Table>, index: usize) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let column = &table.columns[index];
    let at = At {
        row: None,
        column: index,
    };
    let label = cell_text(column.label.clone(), |t| {
        shared::control_text(t, 1.0).with_font_weight(shared::strong_weight())
    })?;
    let mut content = vec![label];
    if column.sortable {
        let sorted = {
            let (table, id) = (table.clone(), column.id.clone());
            move || table.sorted_by(&id)
        };
        let shown = sorted.clone();
        content.push(shared::shown_while(
            move || shown().is_some(),
            LayoutStyle::row(),
            move || {
                let sorted = sorted.clone();
                let point = move || match sorted() {
                    Some(SortDirection::Descending) => shared::Mark::ChevronDown,
                    _ => shared::Mark::ChevronUp,
                };
                let mark = shared::mark(point, |text| text.color)?;
                let spaced = Container::new(
                    LayoutStyle::row()
                        .align_items(AlignItems::CENTER)
                        .padding_start(shared::gap_xs()),
                    vec![mark],
                )?;
                Ok(box_item(spaced))
            },
        )?);
    }
    let laid = signal(None);
    if column.resizable {
        content.push(column_resizer(table, index, laid)?);
    }
    let cell = presented_cell(table, at, index, Role::ColumnHeader, content)?;
    laid.set(track_layout(cell.layout_node()));
    let name = column.label.clone();
    let cell = cell.a11y_label(move || name.clone());
    let cell = if column.sortable {
        let (table, id) = (table.clone(), column.id.clone());
        cell.sorted(move || table.sorted_by(&id))
    } else {
        cell
    };
    Ok(box_item(cell))
}

/// The splitter on the inline-end edge of the header of the column at `index`, whose laid-out box is `laid`: what a drag or a key step resizes the column by. A cancelled drag gives the widths back as they were before it, so a flexible column it took hold of flexes again.
fn column_resizer(
    table: &Rc<Table>,
    index: usize,
    laid: RwSignal<Option<RwSignal<Rect>>>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let column = &table.columns[index];
    let widths = table.widths;
    let held: Rc<RefCell<Option<ColumnWidths>>> = Rc::default();
    let resize = {
        let id = column.id.clone();
        move |size: Size| {
            widths.update(|widths| {
                widths.insert(id.clone(), size.width);
            })
        }
    };
    let start = {
        let held = held.clone();
        move || *held.borrow_mut() = Some(widths.peek())
    };
    let end = move |kept: bool| {
        if let Some(before) = held.take()
            && !kept
        {
            widths.set(before);
        }
    };
    let name = column.label.clone();
    let handle = resize_handle(
        ResizeHandleProps::props()
            .edge(ResizeEdge::End)
            .size(Reactive::of(move || {
                Size::new(laid.get().map_or(0.0, |rect| rect.get().width), 0.0)
            }))
            .on_resize(Rc::new(resize))
            .min(Size::new(column.min_width, 0.0))
            .label(Reactive::of(move || {
                strings::text_with(strings::RESIZE_COLUMN, &[("label", &name)])
            }))
            .on_start(Rc::new(start))
            .on_end(Rc::new(end))
            .build(),
        Children::default(),
    )?;
    let edge = Container::new(
        LayoutStyle::row()
            .absolute()
            .inset_top(0.0)
            .inset_bottom(0.0)
            .inset_end(0.0),
        vec![handle],
    )?;
    Ok(box_item(edge))
}

/// A cell the keyboard's cursor can rest on: announced as `role`, ringed while the cursor is on it, and pressed like the row it is in.
fn presented_cell(
    table: &Rc<Table>,
    at: At,
    index: usize,
    role: Role,
    content: Vec<Box<dyn LayoutItem>>,
) -> Result<StyledContainer, LayoutError> {
    let shape = {
        let (table, header) = (table.clone(), at.row.is_none());
        move || cell_box(&table, index, header)
    };
    let paint = {
        let (table, at) = (table.clone(), at.clone());
        move |_r| {
            cursor_row_paint(
                DataTableTokens::current().radius(),
                &table.color,
                RowState {
                    cursor: table.shows_cursor(&at),
                    ..RowState::default()
                },
            )
        }
    };
    let press = {
        let (table, at) = (table.clone(), at.clone());
        move || table.press(&at)
    };
    let cell = StyledContainer::new(shape(), paint, content)?
        .styled_by(shape)
        .presented(role)
        .on_press(press);
    if let Some(focus_id) = cell.focus_id() {
        table.built.track(at, focus_id);
    }
    Ok(cell)
}

fn body_row(
    table: &Rc<Table>,
    row: TableRow,
    place: SetPosition,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let cells = table
        .columns
        .iter()
        .enumerate()
        .map(|(index, column)| {
            let text = row.cells.get(index).cloned().unwrap_or_default();
            let content = match &column.cell {
                Some(build) => build(&row)?,
                None if column.wrap => wrapped_text(text, column.align)?,
                None => cell_text(text, |t| shared::control_text(t, 1.0))?,
            };
            let at = At {
                row: Some(row.id.clone()),
                column: index,
            };
            presented_cell(table, at, index, Role::GridCell, vec![content]).map(box_item)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let id = row.id.clone();
    let paint = |hovered: bool| {
        let (table, id) = (table.clone(), id.clone());
        move |_r| {
            cursor_row_paint(
                DataTableTokens::current().radius(),
                &table.color,
                RowState {
                    selected: table.selection.is_selected(&id),
                    hovered,
                    cursor: false,
                },
            )
        }
    };
    let selected = {
        let (table, id) = (table.clone(), id.clone());
        move || table.selection.is_selected(&id)
    };
    let height = (!table.places.is_measured()).then_some(table.row_height);
    let row_box = move || line_box(height);
    let built = StyledContainer::new(row_box(), paint(false), cells)?
        .styled_by(row_box)
        .hover_style(paint(true))
        .presented(Role::Row)
        .positioned(place)
        .toggled(selected);
    Ok(box_item(built))
}

#[cfg(test)]
#[path = "data_table_test.rs"]
mod tests;
