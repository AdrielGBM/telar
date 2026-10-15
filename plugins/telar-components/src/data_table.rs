//! [`data_table`]: rows of data under a header of columns — sorted by a column, walked cell by cell from the keyboard, with only the rows on screen built.

use std::cell::Cell;
use std::cmp::Ordering;
use std::rc::Rc;
use std::sync::Arc;

use telar::{
    Accessible, AlignItems, Border, Children, Clip, ClippedItem, Color, Container, JustifyContent,
    Key, KeyNav, KeyNavMove, LayoutError, LayoutItem, LayoutScrollArea, LayoutStyle, Memo,
    NamedKey, Props, Reactive, Rect, RectStyle, RwSignal, ScrollViewport, ShapeStyle,
    SortDirection, StyledContainer, Text, TextWrap, VirtualList, box_item, children,
    current_direction,
    focus::{self, FocusId, Role, SetPosition},
    key_nav_apply_grid, memo, signal, track_layout,
};

use crate::rows::{BuiltRows, RowState, cursor_row_paint};
use crate::shared;

const OVERSCAN: usize = 4;

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

/// One column of a [`data_table`]: its header, its width, where its cells sit, and whether and how it sorts the rows.
#[derive(Clone)]
pub struct Column {
    /// Unique among the table's columns: what a [`TableSort`] names it by.
    pub id: Arc<str>,
    /// The header's text.
    pub label: Arc<str>,
    pub width: ColumnWidth,
    pub align: CellAlign,
    /// Whether a press or Enter on its header sorts the rows by it.
    pub sortable: bool,
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
/// It fills the height its parent gives it and scrolls within it, header held at the top, building only the rows on screen, so a table of thousands of rows costs what a screenful does. Every row stands at [`DataTableTokens::row_height`].
///
/// The keyboard walks a cursor from cell to cell — the arrows, Home and End along a row, Ctrl with them to the table's corners, Page Up and Page Down — and a reader hears the cell it is on. Enter, Space or a press on a row's cell chooses the row; on a sortable column's header they sort the rows by it, and again the other way.
///
/// Columns are not resized by dragging yet.
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
    /// The bound selection, by row id. `None` (the default) is uncontrolled.
    #[props(some, into, default)]
    pub selected: Option<RwSignal<Option<Arc<str>>>>,
    /// Fired with the row a press, Enter or Space chose.
    #[props(some, default)]
    pub on_select: Option<TableRowHandler>,
    /// What a reader calls the table.
    #[props(some, into, default)]
    pub label: Option<Reactive<String>>,
    /// Accent for the selection and the keyboard cursor. `Color::TRANSPARENT` (the default) falls back to the theme's washes and focus ring.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub color: Reactive<Color>,
}

/// Rows of data under a header of sortable columns, walked cell by cell.
pub fn data_table(
    props: DataTableProps,
    _children: Children,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let DataTableProps {
        columns,
        rows,
        sort,
        on_sort,
        sort_rows,
        selected,
        on_select,
        label,
        color,
    } = props;
    let columns: Rc<[Column]> = columns.into();
    let sort = sort.unwrap_or_else(|| signal(None));
    let selected = selected.unwrap_or_else(|| signal(None));
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

    let mut made: Option<Rc<Table>> = None;
    let scroll = LayoutScrollArea::new_with(LayoutStyle::new().flex_grow(1.0), |viewport| {
        let table = Rc::new(Table {
            columns,
            rows: ordered,
            sort,
            on_sort,
            selected,
            on_select,
            cursor: signal(None),
            viewport,
            row_height,
            focus: Cell::new(None),
            built: BuiltRows::new(),
            color,
        });
        made = Some(table.clone());
        let header = header_row(&table)?;
        let source = {
            let table = table.clone();
            move || placed(&table.rows())
        };
        let body = {
            let table = table.clone();
            VirtualList::new(
                LayoutStyle::column(),
                table.viewport.clone(),
                row_height,
                OVERSCAN,
                source,
                |item: &(TableRow, SetPosition)| item.clone(),
                move |_, (row, place)| body_row(&table, row, place),
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
    table.focus.set(scroll.focus_id());
    Ok(match label {
        Some(label) => box_item(scroll.a11y_label(move || label.get())),
        None => box_item(scroll),
    })
}

/// The row on `line` of a grid of `rows` rows under its header, as the grid counts its lines: the header first.
fn line_place(line: usize, rows: usize) -> SetPosition {
    SetPosition {
        level: 1,
        position: line as u32 + 1,
        size: rows as u32 + 1,
    }
}

/// Each row with its place among the grid's, which a reader cannot count from the rows that are built.
fn placed(rows: &[TableRow]) -> Vec<(TableRow, SetPosition)> {
    rows.iter()
        .enumerate()
        .map(|(i, row)| (row.clone(), line_place(i + 1, rows.len())))
        .collect()
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
    selected: RwSignal<Option<Arc<str>>>,
    on_select: Option<TableRowHandler>,
    /// Deliberately not the selection: walking the table chooses nothing.
    cursor: RwSignal<Option<At>>,
    viewport: ScrollViewport,
    /// The header's height too.
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

    fn is_selected(&self, id: &str) -> bool {
        self.selected
            .with(|selected| selected.as_deref() == Some(id))
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

    /// Puts the cursor where Tab arriving at the table should find it: on the selected row, else on the first row, else on the header.
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
            .selected
            .peek()
            .filter(|id| rows.iter().any(|row| &row.id == id))
            .or_else(|| rows.first().map(|row| row.id.clone()));
        self.cursor.set(Some(At { row, column: 0 }));
    }

    fn move_to(&self, rows: &[TableRow], line: usize, column: usize) {
        let at = Self::at_line(rows, line, column);
        self.cursor.set(Some(at.clone()));
        self.reveal(line, &at);
    }

    /// Scrolls the least distance that brings the cursor's cell into view, below the header that stays over the rows: down by the row's place, which is known whether or not it is built, and across by the cell's, once it is.
    fn reveal(&self, line: usize, at: &At) {
        if self.viewport.rect().peek().height <= 0.0 {
            return;
        }
        let (x, y) = self.viewport.peek_offset();
        let across = self
            .built
            .get(at)
            .and_then(focus::node_of)
            .and_then(track_layout)
            .map(|cell| cell.peek());
        let (left, width) = across.map_or((x, 0.0), |cell| (cell.x, cell.width));
        let band = match line {
            0 => Rect::new(left, y, width, 0.0),
            line => Rect::new(
                left,
                (line - 1) as f32 * self.row_height,
                width,
                self.row_height * 2.0,
            ),
        };
        self.viewport.reveal_rect(band, 0.0);
    }

    /// How many rows one Page Up or Page Down moves: as many as the view shows whole under the header.
    fn page(&self) -> usize {
        let height = self.viewport.rect().peek().height - self.row_height;
        ((height / self.row_height).floor() as usize).max(1)
    }

    /// What Enter, Space or a press on the cell at `line` and `column` does: sorts by the column on the header, chooses the row anywhere else.
    fn activate(&self, rows: &[TableRow], line: usize, column: usize) {
        match line {
            0 => self.sort_by(column),
            line => self.choose(&rows[line - 1]),
        }
    }

    fn choose(&self, row: &TableRow) {
        self.selected.set(Some(row.id.clone()));
        if let Some(on_select) = &self.on_select {
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

    fn press(&self, at: &At) {
        let rows = self.rows();
        if let Some((line, column)) = self.line_of(&rows, at) {
            self.cursor.set(Some(at.clone()));
            self.activate(&rows, line, column);
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
        let corner = modifiers.is_ctrl || modifiers.is_meta;
        let (line, column) = match key {
            Key::Named(NamedKey::Enter | NamedKey::Space) => {
                self.activate(&rows, line, column);
                return true;
            }
            Key::Named(NamedKey::Home) if corner => (0, 0),
            Key::Named(NamedKey::End) if corner => (lines - 1, width - 1),
            Key::Named(NamedKey::Home) => (line, 0),
            Key::Named(NamedKey::End) => (line, width - 1),
            Key::Named(NamedKey::PageDown) => ((line + self.page()).min(lines - 1), column),
            Key::Named(NamedKey::PageUp) => (line.saturating_sub(self.page()), column),
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
        self.move_to(&rows, line, column);
        true
    }
}

/// A cell's box in `column`: its width, where its content sits across it, and the space from its edges.
fn cell_box(column: &Column, height: f32) -> LayoutStyle {
    let style = LayoutStyle::row()
        .align_items(AlignItems::CENTER)
        .justify_content(match column.align {
            CellAlign::Start => JustifyContent::START,
            CellAlign::Center => JustifyContent::CENTER,
            CellAlign::End => JustifyContent::END,
        })
        .height(height)
        .min_width(0.0)
        .padding_horizontal(DataTableTokens::current().pad_x());
    match column.width {
        ColumnWidth::Fixed(width) => style.width(width).flex_shrink(0.0),
        ColumnWidth::Flex(share) => style.flex_grow(share).flex_shrink(1.0).flex_basis(0.0),
    }
}

fn line_box(height: f32) -> LayoutStyle {
    LayoutStyle::row()
        .align_items(AlignItems::CENTER)
        .height(height)
        .flex_shrink(0.0)
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

/// The header: one cell naming each column, held at the top of the view while the rows scroll under it, on an opaque surface they cannot show through.
fn header_row(table: &Rc<Table>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let cells = table
        .columns
        .iter()
        .enumerate()
        .map(|(index, column)| header_cell(table, index, column))
        .collect::<Result<Vec<_>, _>>()?;
    let height = table.row_height;
    let row_box = move || line_box(height).sticky().inset_top(0.0);
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

fn header_cell(
    table: &Rc<Table>,
    index: usize,
    column: &Column,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
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
    let cell = presented_cell(table, at, column, Role::ColumnHeader, content)?;
    let cell = if column.sortable {
        let (table, id) = (table.clone(), column.id.clone());
        cell.sorted(move || table.sorted_by(&id))
    } else {
        cell
    };
    Ok(box_item(cell))
}

/// A cell the keyboard's cursor can rest on: announced as `role`, ringed while the cursor is on it, and pressed like the row it is in.
fn presented_cell(
    table: &Rc<Table>,
    at: At,
    column: &Column,
    role: Role,
    content: Vec<Box<dyn LayoutItem>>,
) -> Result<StyledContainer, LayoutError> {
    let height = table.row_height;
    let shape = {
        let column = column.clone();
        move || cell_box(&column, height)
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
            let content = match &column.cell {
                Some(build) => build(&row)?,
                None => cell_text(row.cells.get(index).cloned().unwrap_or_default(), |t| {
                    shared::control_text(t, 1.0)
                })?,
            };
            let at = At {
                row: Some(row.id.clone()),
                column: index,
            };
            presented_cell(table, at, column, Role::GridCell, vec![content]).map(box_item)
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
                    selected: table.is_selected(&id),
                    hovered,
                    cursor: false,
                },
            )
        }
    };
    let selected = {
        let (table, id) = (table.clone(), id.clone());
        move || table.is_selected(&id)
    };
    let height = table.row_height;
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
