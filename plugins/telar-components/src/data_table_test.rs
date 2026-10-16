use std::cell::RefCell;
use std::collections::BTreeSet;

use telar::testing::{centre, lay_out, named, press, release, route};

use crate::selection::SelectionMode;
use telar::{AccessNode, ComponentList, Event, Rect, relayout_if_dirty};

use super::*;

fn columns() -> Vec<Column> {
    vec![
        Column::new("name", "Name").sortable(true),
        Column::new("size", "Size")
            .fixed(80.0)
            .align(CellAlign::End)
            .sortable(true),
        Column::new("kind", "Kind"),
    ]
}

fn files() -> Vec<TableRow> {
    vec![
        TableRow::new("b", ["beta.txt", "12", "Text"]),
        TableRow::new("a", ["Alpha.rs", "9", "Rust"]),
        TableRow::new("c", ["gamma.png", "100", "Image"]),
    ]
}

fn many(count: usize) -> Vec<TableRow> {
    (0..count)
        .map(|i| {
            TableRow::new(
                format!("r{i}"),
                [format!("Row {i}"), i.to_string(), "x".into()],
            )
        })
        .collect()
}

struct Mounted {
    tree: ComponentList,
    sort: RwSignal<Option<TableSort>>,
    selected: RwSignal<Option<Arc<str>>>,
    log: Rc<RefCell<Vec<String>>>,
}

impl Mounted {
    fn new(rows: Vec<TableRow>, height: f32) -> Self {
        Self::build(rows, height, true)
    }

    fn build(rows: Vec<TableRow>, height: f32, sort_rows: bool) -> Self {
        crate::test_support::fresh_layout_runtime();
        telar::focus::clear();
        let sort = signal(None);
        let selected = signal(None);
        let log = Rc::new(RefCell::new(Vec::new()));
        let (sorted, chosen) = (log.clone(), log.clone());
        let item = data_table(
            DataTableProps::props()
                .columns(columns())
                .rows(rows)
                .sort(sort)
                .sort_rows(sort_rows)
                .on_sort(Rc::new(move |sort: &TableSort| {
                    sorted.borrow_mut().push(format!(
                        "sort {} {}",
                        sort.column,
                        sort.direction.as_str()
                    ))
                }))
                .selected(selected)
                .on_select(Rc::new(move |row: &TableRow| {
                    chosen.borrow_mut().push(format!("select {}", row.id))
                }))
                .label("Files")
                .build(),
            Children::default(),
        )
        .unwrap();
        lay_out(item.layout_node(), 400.0, height);
        let tree = ComponentList::new(item);
        let mut mounted = Self {
            tree,
            sort,
            selected,
            log,
        };
        mounted.settle();
        mounted
    }

    fn focused(rows: Vec<TableRow>, height: f32) -> Self {
        let mut mounted = Self::new(rows, height);
        let grid = telar::focus::exposed()
            .into_iter()
            .find(|exposed| exposed.role == Role::Grid)
            .expect("a grid");
        telar::focus::request(grid.id);
        mounted.settle();
        mounted
    }

    fn settle(&mut self) {
        relayout_if_dirty();
        let _ = self.tree.commands();
        relayout_if_dirty();
        let _ = self.tree.commands();
    }

    fn send(&mut self, event: Event) {
        route(&mut self.tree, &event);
        self.settle();
    }

    fn key(&mut self, key: NamedKey) {
        self.send(named(key));
    }

    fn read(&self) -> Vec<AccessNode> {
        ui_core::accessibility::snapshot(&self.tree.commands())
    }

    fn of_role(&self, role: Role) -> Vec<AccessNode> {
        self.read()
            .into_iter()
            .filter(|node| node.role == role)
            .collect()
    }

    fn names(&self, role: Role) -> Vec<String> {
        self.of_role(role)
            .into_iter()
            .map(|node| node.name)
            .collect()
    }

    fn first_column(&self) -> Vec<String> {
        self.names(Role::GridCell).into_iter().step_by(3).collect()
    }

    fn header(&self, name: &str) -> AccessNode {
        self.of_role(Role::ColumnHeader)
            .into_iter()
            .find(|node| node.name == name)
            .unwrap_or_else(|| panic!("a `{name}` header"))
    }

    fn pointed(&self) -> Option<String> {
        let nodes = self.read();
        let grid = nodes.iter().find(|node| node.role == Role::Grid)?;
        let cell = grid.active_descendant?;
        nodes
            .iter()
            .find(|node| node.id == Some(cell))
            .map(|node| node.name.clone())
    }

    fn click(&mut self, rect: Rect) {
        let (x, y) = centre(rect);
        self.send(press(x, y));
        self.send(release(x, y));
    }
}

#[test]
fn a_table_is_a_named_grid_of_rows_under_its_column_headers() {
    let mounted = Mounted::new(files(), 300.0);
    let grid = mounted.of_role(Role::Grid);
    assert_eq!(grid.len(), 1);
    assert_eq!(grid[0].name, "Files");
    assert_eq!(mounted.names(Role::ColumnHeader), ["Name", "Size", "Kind"]);
    assert_eq!(mounted.of_role(Role::Row).len(), 3, "a row per row of data");
    assert_eq!(
        mounted.first_column(),
        ["beta.txt", "Alpha.rs", "gamma.png"],
        "unsorted, the rows stand as given"
    );
}

#[test]
fn a_press_on_a_sortable_header_sorts_by_it_and_a_second_turns_it_round() {
    let mut mounted = Mounted::new(files(), 300.0);
    assert_eq!(mounted.header("Name").sort, None);
    mounted.click(mounted.header("Name").rect);
    assert_eq!(
        mounted.first_column(),
        ["Alpha.rs", "beta.txt", "gamma.png"]
    );
    assert_eq!(
        mounted.header("Name").sort,
        Some(SortDirection::Ascending),
        "a reader is told which way"
    );
    assert_eq!(mounted.header("Size").sort, None);
    mounted.click(mounted.header("Name").rect);
    assert_eq!(
        mounted.first_column(),
        ["gamma.png", "beta.txt", "Alpha.rs"]
    );
    assert_eq!(
        *mounted.log.borrow(),
        ["sort name ascending", "sort name descending"]
    );
}

#[test]
fn figures_sort_as_numbers() {
    let mut mounted = Mounted::new(files(), 300.0);
    mounted.click(mounted.header("Size").rect);
    assert_eq!(
        mounted.first_column(),
        ["Alpha.rs", "beta.txt", "gamma.png"],
        "9 before 12 before 100"
    );
}

#[test]
fn a_column_that_is_not_sortable_ignores_a_press() {
    let mut mounted = Mounted::new(files(), 300.0);
    mounted.click(mounted.header("Kind").rect);
    assert_eq!(mounted.sort.get(), None);
    assert!(mounted.log.borrow().is_empty());
}

#[test]
fn rows_the_application_orders_itself_are_shown_as_given() {
    let mut mounted = Mounted::build(files(), 300.0, false);
    mounted.click(mounted.header("Name").rect);
    assert_eq!(*mounted.log.borrow(), ["sort name ascending"]);
    assert_eq!(
        mounted.first_column(),
        ["beta.txt", "Alpha.rs", "gamma.png"]
    );
}

#[test]
fn the_arrows_walk_a_cursor_from_cell_to_cell_and_enter_chooses_its_row() {
    let mut mounted = Mounted::focused(files(), 300.0);
    assert_eq!(mounted.pointed().as_deref(), Some("beta.txt"));
    mounted.key(NamedKey::ArrowRight);
    assert_eq!(mounted.pointed().as_deref(), Some("12"));
    mounted.key(NamedKey::ArrowDown);
    assert_eq!(mounted.pointed().as_deref(), Some("9"));
    mounted.key(NamedKey::End);
    assert_eq!(mounted.pointed().as_deref(), Some("Rust"));
    mounted.key(NamedKey::Home);
    assert_eq!(mounted.pointed().as_deref(), Some("Alpha.rs"));
    assert_eq!(mounted.selected.get(), None, "walking chooses nothing");
    mounted.key(NamedKey::Enter);
    assert_eq!(mounted.selected.get().as_deref(), Some("a"));
    assert_eq!(*mounted.log.borrow(), ["select a"]);
    let chosen: Vec<bool> = mounted
        .of_role(Role::Row)
        .into_iter()
        .map(|row| row.toggled == Some(true))
        .collect();
    assert_eq!(chosen, [false, true, false], "the chosen row says so");
}

#[test]
fn up_from_the_first_row_reaches_the_header_where_enter_sorts() {
    let mut mounted = Mounted::focused(files(), 300.0);
    mounted.key(NamedKey::ArrowUp);
    assert_eq!(mounted.pointed().as_deref(), Some("Name"));
    mounted.key(NamedKey::Enter);
    assert_eq!(mounted.header("Name").sort, Some(SortDirection::Ascending));
    assert_eq!(mounted.pointed().as_deref(), Some("Name"));
}

#[test]
fn only_a_screenful_of_thousands_of_rows_is_built_and_the_header_stays_on_top() {
    let mut mounted = Mounted::focused(many(5_000), 240.0);
    let row_height = DataTableTokens::current().row_height();
    let screenful = (240.0 / row_height).ceil() as usize + 1;
    let built = mounted.of_role(Role::Row).len();
    assert!(
        built <= screenful + OVERSCAN * 2,
        "{built} rows built for a view of {screenful}"
    );
    let top = mounted.header("Name").rect.y;
    mounted.key(NamedKey::PageDown);
    mounted.key(NamedKey::PageDown);
    mounted.key(NamedKey::PageDown);
    let page = ((240.0 - row_height) / row_height).floor() as usize;
    let expected = format!("Row {}", page * 3);
    assert_eq!(mounted.pointed().as_deref(), Some(expected.as_str()));
    assert_eq!(
        mounted.header("Name").rect.y,
        top,
        "the header is held at the top while the rows scroll"
    );
    let cursor = mounted
        .of_role(Role::GridCell)
        .into_iter()
        .find(|cell| cell.name == expected)
        .expect("the cursor's row is built");
    assert!(
        cursor.rect.y >= top + row_height - 0.5,
        "and the cursor's row is below it, not under it"
    );
    let ctrl = telar::ModifiersState {
        is_ctrl: true,
        ..Default::default()
    };
    mounted.send(Event::ModifiersChanged { modifiers: ctrl });
    mounted.send(telar::testing::key_with(Key::Named(NamedKey::End), ctrl));
    mounted.send(Event::ModifiersChanged {
        modifiers: Default::default(),
    });
    assert_eq!(
        mounted.pointed().as_deref(),
        Some("x"),
        "the last cell of the last row"
    );
}

#[test]
fn the_corner_of_the_cursor_follows_the_role_until_the_tokens_name_one() {
    let radius = || DataTableTokens::current().radius();
    telar::set_theme(crate::test_support::TokensTheme::<DataTableTokens> {
        role: 7.0,
        tokens: None,
    });
    assert_eq!(radius(), 7.0);
    telar::set_theme(crate::test_support::TokensTheme {
        role: 7.0,
        tokens: Some(DataTableTokens {
            radius: Some(2.0),
            ..Default::default()
        }),
    });
    assert_eq!(radius(), 2.0);
}

/// Every row built says its index among all the grid's rows, the header first, so a reader counts the table rather than the screenful.
#[test]
fn each_row_built_says_its_index_among_all_the_rows() {
    let mut mounted = Mounted::focused(many(500), 240.0);
    let indexes = |mounted: &Mounted| -> Vec<Option<u32>> {
        mounted
            .of_role(Role::Row)
            .into_iter()
            .map(|row| row.position.map(|place| place.position))
            .collect()
    };
    let sizes: Vec<Option<u32>> = mounted
        .of_role(Role::Row)
        .into_iter()
        .map(|row| row.position.map(|place| place.size))
        .collect();
    assert!(sizes.iter().all(|size| *size == Some(501)), "{sizes:?}");
    assert_eq!(
        indexes(&mounted)[0],
        Some(2),
        "the first row is under the header"
    );
    for _ in 0..100 {
        mounted.key(NamedKey::PageDown);
    }
    assert_eq!(indexes(&mounted).last(), Some(&Some(501)));
}

struct Grid {
    tree: ComponentList,
}

impl Grid {
    fn mount(props: DataTableProps, children: Children) -> Self {
        crate::test_support::fresh_layout_runtime();
        ui_core::reset_keyboard();
        telar::focus::clear();
        telar::set_keyboard_convention(telar::KeyboardConvention::Pc);
        let item = data_table(props, children).unwrap();
        lay_out(item.layout_node(), 400.0, 300.0);
        let mut grid = Self {
            tree: ComponentList::new(item),
        };
        grid.settle();
        grid
    }

    fn settle(&mut self) {
        relayout_if_dirty();
        let _ = self.tree.commands();
        relayout_if_dirty();
        let _ = self.tree.commands();
    }

    fn send(&mut self, event: Event) {
        route(&mut self.tree, &event);
        self.settle();
    }

    fn holding(&mut self, modifiers: telar::ModifiersState, act: impl FnOnce(&mut Self)) {
        self.send(Event::ModifiersChanged { modifiers });
        act(self);
        self.send(Event::ModifiersChanged {
            modifiers: Default::default(),
        });
    }

    fn key(&mut self, key: Key, modifiers: telar::ModifiersState) {
        self.holding(modifiers, |grid| {
            grid.send(telar::testing::key_with(key, modifiers))
        });
    }

    fn read(&self) -> Vec<AccessNode> {
        ui_core::accessibility::snapshot(&self.tree.commands())
    }

    fn find(&self, role: Role, name: &str) -> AccessNode {
        self.read()
            .into_iter()
            .find(|node| node.role == role && node.name == name)
            .unwrap_or_else(|| panic!("a {role:?} named `{name}`"))
    }

    fn names(&self, role: Role) -> Vec<String> {
        self.read()
            .into_iter()
            .filter(|node| node.role == role)
            .map(|node| node.name)
            .collect()
    }

    fn width_of(&self, header: &str) -> f32 {
        self.find(Role::ColumnHeader, header).rect.width
    }

    fn click(&mut self, role: Role, name: &str) {
        let (x, y) = centre(self.find(role, name).rect);
        self.send(press(x, y));
        self.send(release(x, y));
    }

    fn drag(&mut self, splitter: &str, by: f64) {
        let (x, y) = centre(self.find(Role::Splitter, splitter).rect);
        for event in [
            press(x, y),
            telar::testing::moved(x + by, y),
            release(x + by, y),
        ] {
            self.send(event);
        }
    }

    fn focus(&mut self, role: Role) {
        let id = telar::focus::exposed()
            .into_iter()
            .find(|exposed| exposed.role == role)
            .unwrap_or_else(|| panic!("a {role:?}"))
            .id;
        telar::focus::request(id);
        self.settle();
    }

    fn focused(&self) -> Option<AccessNode> {
        self.read().into_iter().find(|node| node.focused)
    }

    fn chosen_rows(&self) -> Vec<bool> {
        self.read()
            .into_iter()
            .filter(|node| node.role == Role::Row)
            .map(|node| node.toggled == Some(true))
            .collect()
    }
}

fn shift() -> telar::ModifiersState {
    telar::ModifiersState {
        is_shift: true,
        ..Default::default()
    }
}

fn ctrl() -> telar::ModifiersState {
    telar::ModifiersState {
        is_ctrl: true,
        ..Default::default()
    }
}

fn plain() -> telar::ModifiersState {
    telar::ModifiersState::default()
}

fn resizable_columns() -> Vec<Column> {
    vec![
        Column::new("name", "Name")
            .flex(2.0)
            .sortable(true)
            .resizable(true)
            .min_width(100.0),
        Column::new("size", "Size")
            .fixed(80.0)
            .align(CellAlign::End)
            .resizable(true),
        Column::new("kind", "Kind"),
    ]
}

fn resizable(widths: RwSignal<ColumnWidths>) -> Grid {
    Grid::mount(
        DataTableProps::props()
            .columns(resizable_columns())
            .rows(files())
            .widths(widths)
            .label("Files")
            .build(),
        Children::default(),
    )
}

/// A resizable column's header holds a splitter named for the column and reading its width and narrowest, and the header is still named by its column alone.
#[test]
fn a_resizable_column_s_header_holds_a_splitter_that_reads_its_width() {
    let grid = resizable(signal(ColumnWidths::new()));
    assert_eq!(
        grid.names(Role::Splitter),
        ["Resize Name column", "Resize Size column"],
        "a column that is not resizable has none"
    );
    assert_eq!(grid.names(Role::ColumnHeader), ["Name", "Size", "Kind"]);
    let size = grid.find(Role::Splitter, "Resize Size column");
    let value = size.value.expect("a splitter reads a value");
    assert_eq!((value.now, value.min), (80.0, MIN_COLUMN_WIDTH as f64));
    assert_eq!(size.orientation, Some(telar::Orientation::Vertical));
    let name = grid.find(Role::Splitter, "Resize Name column");
    assert_eq!(name.value.map(|value| value.min), Some(100.0));
    assert_eq!(
        name.value.map(|value| value.now as f32),
        Some(grid.width_of("Name"))
    );
}

/// A drag on a header's edge resizes its column and every cell under it, a flexible column it takes hold of stands fixed after, and the flexible columns left share what is left.
#[test]
fn dragging_a_header_s_edge_resizes_its_column_and_the_cells_under_it() {
    let widths = signal(ColumnWidths::new());
    let mut grid = resizable(widths);
    let (name, kind) = (grid.width_of("Name"), grid.width_of("Kind"));
    grid.drag("Resize Size column", 40.0);
    assert_eq!(grid.width_of("Size"), 120.0);
    assert_eq!(grid.find(Role::GridCell, "12").rect.width, 120.0);
    assert_eq!(widths.get().get("size").copied(), Some(120.0));
    assert!(
        (grid.width_of("Name") + grid.width_of("Kind") - (name + kind - 40.0)).abs() < 1.0,
        "the flexible columns give up the room"
    );

    let before = grid.width_of("Name");
    grid.drag("Resize Name column", -30.0);
    assert_eq!(grid.width_of("Name"), before - 30.0);
    assert_eq!(widths.get().get("name").copied(), Some(before - 30.0));
    assert!(
        (grid.width_of("Kind")
            - (400.0 - ScrollbarStyle::default().width - 120.0 - (before - 30.0)))
            .abs()
            < 1.0
    );
    grid.drag("Resize Name column", -500.0);
    assert_eq!(grid.width_of("Name"), 100.0, "never under its narrowest");
}

#[test]
fn escape_puts_a_drag_back_and_a_flexible_column_flexes_again() {
    let widths = signal(ColumnWidths::new());
    let mut grid = resizable(widths);
    let name = grid.width_of("Name");
    let (x, y) = centre(grid.find(Role::Splitter, "Resize Name column").rect);
    grid.send(press(x, y));
    grid.send(telar::testing::moved(x - 40.0, y));
    assert_eq!(grid.width_of("Name"), name - 40.0);
    grid.send(named(NamedKey::Escape));
    assert_eq!(grid.width_of("Name"), name);
    assert!(widths.get().is_empty(), "{:?}", widths.get());
}

/// Each splitter is a Tab stop after the table's own, stepped by the arrows, and Home takes its column to its narrowest.
#[test]
fn a_focused_splitter_is_stepped_by_the_keys() {
    let widths = signal(ColumnWidths::new());
    let mut grid = resizable(widths);
    grid.focus(Role::Grid);
    telar::focus::focus_next();
    grid.settle();
    assert_eq!(
        grid.focused().map(|node| node.name).as_deref(),
        Some("Resize Name column")
    );
    let name = grid.width_of("Name");
    grid.key(Key::Named(NamedKey::ArrowRight), plain());
    assert_eq!(grid.width_of("Name"), name + 16.0);
    grid.key(Key::Named(NamedKey::Home), plain());
    assert_eq!(grid.width_of("Name"), 100.0);
    grid.key(Key::Named(NamedKey::ArrowLeft), plain());
    assert_eq!(widths.get().get("name").copied(), Some(100.0));
    telar::focus::focus_next();
    grid.settle();
    assert_eq!(
        grid.focused().map(|node| node.name).as_deref(),
        Some("Resize Size column")
    );
}

#[test]
fn bound_widths_lay_the_columns_out_and_hear_every_resize() {
    let widths = signal(ColumnWidths::from([(Arc::from("size"), 150.0)]));
    let mut grid = resizable(widths);
    assert_eq!(grid.width_of("Size"), 150.0);
    widths.set(ColumnWidths::from([(Arc::from("size"), 20.0)]));
    grid.settle();
    assert_eq!(
        grid.width_of("Size"),
        MIN_COLUMN_WIDTH,
        "a bound width under the narrowest stands at the narrowest"
    );
}

struct Chosen {
    grid: Grid,
    rows: RwSignal<BTreeSet<Arc<str>>>,
    heard: Rc<RefCell<Vec<String>>>,
}

fn several() -> Chosen {
    let rows = signal(BTreeSet::new());
    let heard = Rc::new(RefCell::new(Vec::new()));
    let (changes, picks) = (heard.clone(), heard.clone());
    let grid = Grid::mount(
        DataTableProps::props()
            .columns(columns())
            .rows(files())
            .selection_mode(SelectionMode::Multiple)
            .selected_rows(rows)
            .on_selection_change(Rc::new(move |rows: &BTreeSet<Arc<str>>| {
                changes.borrow_mut().push(format!("change {}", rows.len()))
            }))
            .on_select(Rc::new(move |row: &TableRow| {
                picks.borrow_mut().push(format!("select {}", row.id))
            }))
            .label("Files")
            .build(),
        Children::default(),
    );
    Chosen { grid, rows, heard }
}

impl Chosen {
    fn chosen(&self) -> Vec<String> {
        self.rows.get().iter().map(|id| id.to_string()).collect()
    }
}

#[test]
fn a_table_of_several_choices_is_multiselectable_and_each_row_says_if_it_is_chosen() {
    let mut table = several();
    let grid = table.grid.find(Role::Grid, "Files");
    assert!(grid.multiselectable);
    table.grid.click(Role::GridCell, "beta.txt");
    table
        .grid
        .holding(ctrl(), |grid| grid.click(Role::GridCell, "Image"));
    assert_eq!(table.chosen(), ["b", "c"]);
    assert_eq!(table.grid.chosen_rows(), [true, false, true]);
    assert_eq!(
        *table.heard.borrow(),
        ["change 1", "select b", "change 2"],
        "a plain press chooses its row on its own; a Ctrl press only adds"
    );
    table
        .grid
        .holding(shift(), |grid| grid.click(Role::GridCell, "Alpha.rs"));
    assert_eq!(
        table.chosen(),
        ["a", "c"],
        "Shift runs from the row pressed last to this one"
    );
    let single = Mounted::new(files(), 300.0);
    assert!(!single.of_role(Role::Grid)[0].multiselectable);
}

#[test]
fn space_toggles_shift_with_the_arrows_extends_and_ctrl_a_takes_every_row() {
    let mut table = several();
    table.grid.focus(Role::Grid);
    table.grid.key(Key::Named(NamedKey::Space), plain());
    assert_eq!(table.chosen(), ["b"]);
    table.grid.key(Key::Named(NamedKey::ArrowDown), shift());
    table.grid.key(Key::Named(NamedKey::ArrowDown), shift());
    assert_eq!(table.chosen(), ["a", "b", "c"]);
    table.grid.key(Key::Named(NamedKey::ArrowUp), shift());
    assert_eq!(table.chosen(), ["a", "b"]);
    table.grid.key(Key::Named(NamedKey::ArrowRight), plain());
    table.grid.key(Key::Named(NamedKey::Space), plain());
    assert_eq!(table.chosen(), ["b"], "Space took the cursor's row away");
    table.grid.key(Key::Char('a'), ctrl());
    assert_eq!(table.chosen(), ["a", "b", "c"]);
    table.grid.key(Key::Char('a'), ctrl());
    assert!(table.chosen().is_empty(), "and none once all are");
    table.grid.key(Key::Named(NamedKey::Enter), plain());
    assert_eq!(table.chosen(), ["a"], "Enter chooses the row on its own");
    table.grid.key(Key::Named(NamedKey::Home), ctrl());
    table.grid.key(Key::Named(NamedKey::Space), plain());
    assert_eq!(
        table.grid.find(Role::ColumnHeader, "Name").sort,
        Some(SortDirection::Ascending),
        "Space on a header still sorts"
    );
    assert_eq!(table.chosen(), ["a"]);
}

#[test]
fn an_empty_table_keeps_its_header_and_says_so_under_it_until_rows_arrive() {
    let rows = signal(Vec::<TableRow>::new());
    let mut grid = Grid::mount(
        DataTableProps::props()
            .columns(columns())
            .rows(Reactive::of(move || rows.get()))
            .label("Files")
            .build(),
        Children::default(),
    );
    assert_eq!(grid.names(Role::ColumnHeader), ["Name", "Size", "Kind"]);
    let header = grid.find(Role::ColumnHeader, "Name").rect;
    let message = grid.find(Role::Label, "Nothing to show");
    assert!(
        message.rect.y >= header.y + header.height,
        "under the header"
    );
    grid.focus(Role::Grid);
    grid.key(Key::Named(NamedKey::ArrowDown), plain());
    grid.key(Key::Named(NamedKey::Space), plain());
    rows.set(files());
    grid.settle();
    assert!(
        grid.names(Role::Label).is_empty(),
        "{:?}",
        grid.names(Role::Label)
    );
    assert_eq!(grid.names(Role::Row).len(), 3);
}

#[test]
fn an_empty_table_shows_its_own_words_or_its_children() {
    let grid = Grid::mount(
        DataTableProps::props()
            .columns(columns())
            .empty_label("No files yet")
            .build(),
        Children::default(),
    );
    assert_eq!(grid.names(Role::Label), ["No files yet"]);
    let grid = Grid::mount(
        DataTableProps::props().columns(columns()).build(),
        Children::new(|| {
            let mut slots = telar::Slots::new();
            let text =
                Text::declaring(|| "Drop files here".to_string(), LayoutStyle::new(), |t| t)?;
            slots.push(None, box_item(text));
            Ok(slots)
        }),
    );
    assert_eq!(grid.names(Role::Label), ["Drop files here"]);
}
