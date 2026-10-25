use std::collections::BTreeSet;
use telar::testing::{key_with, lay_out, named, press, release, route};

use telar::{AccessNode, ComponentList, DrawCommand, Event, Slots, relayout_if_dirty};

use super::*;

fn rows(count: usize) -> Vec<ListRow> {
    (0..count)
        .map(|i| ListRow::new(format!("row-{i}"), format!("Row {i}")))
        .collect()
}

fn fruit() -> Vec<ListRow> {
    vec![
        ListRow::new("apple", "Apple").with_detail("3"),
        ListRow::new("banana", "Banana").disabled(true),
        ListRow::new("blueberry", "Blueberry"),
        ListRow::new("cherry", "Cherry"),
    ]
}

struct Mounted {
    tree: ComponentList,
    selected: RwSignal<Option<Arc<str>>>,
    picked: Rc<RefCell<Vec<String>>>,
}

impl Mounted {
    fn new(items: Vec<ListRow>, height: f32) -> Self {
        Self::mount(items, height, false)
    }

    fn wrapping(items: Vec<ListRow>, height: f32) -> Self {
        let mounted = Self::mount(items, height, true);
        mounted.settle();
        mounted
    }

    fn mount(items: Vec<ListRow>, height: f32, wrap: bool) -> Self {
        crate::test_support::fresh_layout_runtime();
        telar::focus::clear();
        let selected = signal(None);
        let picked = Rc::new(RefCell::new(Vec::new()));
        let sink = picked.clone();
        let item = virtual_list(
            VirtualListProps::props()
                .items(items)
                .selected(selected)
                .on_select(Rc::new(move |row: &ListRow| {
                    sink.borrow_mut().push(row.id.to_string())
                }))
                .label("Fruit")
                .wrap(wrap)
                .build(),
            Children::default(),
        )
        .unwrap();
        lay_out(item.layout_node(), 240.0, height);
        let tree = ComponentList::new(item);
        let _ = tree.commands();
        Self {
            tree,
            selected,
            picked,
        }
    }

    fn focused(items: Vec<ListRow>, height: f32) -> Self {
        let mounted = Self::new(items, height);
        let list = telar::focus::exposed()
            .into_iter()
            .find(|exposed| exposed.role == Role::ListBox)
            .expect("a list box");
        telar::focus::request(list.id);
        relayout_if_dirty();
        let _ = mounted.tree.commands();
        mounted
    }

    fn send(&mut self, event: Event) {
        route(&mut self.tree, &event);
        relayout_if_dirty();
        let _ = self.tree.commands();
    }

    fn key(&mut self, key: NamedKey) {
        self.send(named(key));
    }

    fn settle(&self) {
        for _ in 0..8 {
            relayout_if_dirty();
            let _ = self.tree.commands();
        }
    }

    fn read(&self) -> Vec<AccessNode> {
        ui_core::accessibility::snapshot(&self.tree.commands())
    }

    fn pointed(&self) -> Option<String> {
        let nodes = self.read();
        let list = nodes.iter().find(|node| node.role == Role::ListBox)?;
        let row = list.active_descendant?;
        nodes
            .iter()
            .find(|node| node.id == Some(row))
            .map(|node| node.name.clone())
    }

    fn drawn(&self) -> Vec<String> {
        self.tree
            .commands()
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Text { text, .. } => Some(text.to_string()),
                _ => None,
            })
            .collect()
    }

    fn selected(&self) -> Option<String> {
        self.selected.get().as_deref().map(str::to_string)
    }
}

#[test]
fn a_list_is_a_named_list_box_of_options() {
    let mounted = Mounted::new(fruit(), 300.0);
    let nodes = mounted.read();
    let list = nodes
        .iter()
        .find(|node| node.role == Role::ListBox)
        .expect("a list box");
    assert_eq!(list.name, "Fruit");
    let options: Vec<&str> = nodes
        .iter()
        .filter(|node| node.role == Role::Option)
        .map(|node| node.name.as_str())
        .collect();
    assert_eq!(options, ["Apple 3", "Banana", "Blueberry", "Cherry"]);
}

#[test]
fn only_a_screenful_of_ten_thousand_rows_is_built() {
    let mounted = Mounted::new(rows(10_000), 200.0);
    let built = mounted.drawn().len();
    let screenful = (200.0 / ListTokens::current().row_height()).ceil() as usize + 1;
    assert!(
        built <= screenful + OVERSCAN * 2,
        "{built} rows built for a view of {screenful}"
    );
}

#[test]
fn the_arrows_walk_a_cursor_past_disabled_rows_and_enter_chooses() {
    let mut mounted = Mounted::focused(fruit(), 300.0);
    assert_eq!(mounted.pointed().as_deref(), Some("Apple 3"));
    mounted.key(NamedKey::ArrowDown);
    assert_eq!(
        mounted.pointed().as_deref(),
        Some("Blueberry"),
        "the disabled banana is passed over"
    );
    assert_eq!(mounted.selected(), None, "walking chooses nothing");
    mounted.key(NamedKey::Enter);
    assert_eq!(mounted.selected().as_deref(), Some("blueberry"));
    assert_eq!(*mounted.picked.borrow(), ["blueberry"]);
    mounted.key(NamedKey::End);
    assert_eq!(mounted.pointed().as_deref(), Some("Cherry"));
    mounted.key(NamedKey::ArrowDown);
    assert_eq!(
        mounted.pointed().as_deref(),
        Some("Apple 3"),
        "the cursor wraps from the end to the start"
    );
    mounted.key(NamedKey::Space);
    assert_eq!(mounted.selected().as_deref(), Some("apple"));
}

#[test]
fn typing_jumps_to_the_row_a_label_starts_with() {
    let mut mounted = Mounted::focused(fruit(), 300.0);
    mounted.send(key_with(Key::Char('c'), Default::default()));
    assert_eq!(mounted.pointed().as_deref(), Some("Cherry"));
}

#[test]
fn a_press_chooses_a_row_and_a_disabled_one_cannot_be_chosen() {
    let mut mounted = Mounted::new(fruit(), 300.0);
    let rect = |name: &str, mounted: &Mounted| {
        mounted
            .read()
            .into_iter()
            .find(|node| node.role == Role::Option && node.name == name)
            .unwrap_or_else(|| panic!("{name}"))
            .rect
    };
    let cherry = rect("Cherry", &mounted);
    let (x, y) = telar::testing::centre(cherry);
    mounted.send(press(x, y));
    mounted.send(release(x, y));
    assert_eq!(mounted.selected().as_deref(), Some("cherry"));
    let banana = rect("Banana", &mounted);
    let (x, y) = telar::testing::centre(banana);
    mounted.send(press(x, y));
    mounted.send(release(x, y));
    assert_eq!(mounted.selected().as_deref(), Some("cherry"));
}

#[test]
fn page_down_moves_a_view_s_worth_and_scrolls_the_cursor_into_view() {
    let mut mounted = Mounted::focused(rows(200), 200.0);
    let page = (200.0 / ListTokens::current().row_height()).floor() as usize;
    mounted.key(NamedKey::PageDown);
    let expected = format!("Row {page}");
    assert_eq!(mounted.pointed().as_deref(), Some(expected.as_str()));
    mounted.key(NamedKey::End);
    assert_eq!(
        mounted.pointed().as_deref(),
        Some("Row 199"),
        "the last row is built once the cursor scrolls to it"
    );
    mounted.key(NamedKey::Home);
    assert_eq!(mounted.pointed().as_deref(), Some("Row 0"));
}

#[test]
fn a_row_builder_draws_the_row_while_the_list_keeps_it_a_choosable_option() {
    crate::test_support::fresh_layout_runtime();
    let item = virtual_list(
        VirtualListProps::props()
            .items(fruit())
            .row(Rc::new(|row: &ListRow| {
                let shout = row.label.to_uppercase();
                let text = Text::declaring(move || shout.clone(), LayoutStyle::new(), |t| t)?;
                Ok(box_item(text))
            }))
            .build(),
        Children::default(),
    )
    .unwrap();
    lay_out(item.layout_node(), 240.0, 300.0);
    let tree = ComponentList::new(item);
    let options: Vec<String> = ui_core::accessibility::snapshot(&tree.commands())
        .into_iter()
        .filter(|node| node.role == Role::Option)
        .map(|node| node.name)
        .collect();
    assert_eq!(options, ["APPLE", "BANANA", "BLUEBERRY", "CHERRY"]);
}

/// Every option built says where it sits among all the rows, the ones never built included, so a reader counts the list rather than the screenful: scrolled to the end, the last option is the last of them all.
#[test]
fn each_option_built_says_its_place_among_all_the_rows() {
    let mut mounted = Mounted::focused(rows(200), 120.0);
    let places = |mounted: &Mounted| -> Vec<(String, Option<SetPosition>)> {
        mounted
            .read()
            .into_iter()
            .filter(|node| node.role == Role::Option)
            .map(|node| (node.name, node.position))
            .collect()
    };
    let at = |position: u32| {
        Some(SetPosition {
            level: 1,
            position,
            size: 200,
        })
    };
    let first = places(&mounted);
    assert!(first.len() < 200, "only a screenful is built");
    assert_eq!(first[0], ("Row 0".to_string(), at(1)));
    mounted.key(NamedKey::End);
    let last = places(&mounted);
    assert_eq!(last.last(), Some(&("Row 199".to_string(), at(200))));
}

struct Several {
    tree: ComponentList,
    chosen: RwSignal<BTreeSet<Arc<str>>>,
    heard: Rc<RefCell<Vec<String>>>,
}

impl Several {
    fn new(items: Vec<ListRow>) -> Self {
        Self::mount(items, false)
    }

    fn mount(items: Vec<ListRow>, wrap: bool) -> Self {
        crate::test_support::fresh_layout_runtime();
        ui_core::reset_keyboard();
        telar::focus::clear();
        telar::set_keyboard_convention(telar::KeyboardConvention::Pc);
        let chosen = signal(BTreeSet::new());
        let heard = Rc::new(RefCell::new(Vec::new()));
        let (changes, picks) = (heard.clone(), heard.clone());
        let item = virtual_list(
            VirtualListProps::props()
                .items(items)
                .selection_mode(SelectionMode::Multiple)
                .selected_rows(chosen)
                .on_selection_change(Rc::new(move |rows: &BTreeSet<Arc<str>>| {
                    changes.borrow_mut().push(format!("change {}", rows.len()))
                }))
                .on_select(Rc::new(move |row: &ListRow| {
                    picks.borrow_mut().push(format!("select {}", row.id))
                }))
                .label("Fruit")
                .wrap(wrap)
                .build(),
            Children::default(),
        )
        .unwrap();
        lay_out(item.layout_node(), 240.0, 300.0);
        let tree = ComponentList::new(item);
        let _ = tree.commands();
        Self {
            tree,
            chosen,
            heard,
        }
    }

    fn send(&mut self, event: Event) {
        route(&mut self.tree, &event);
        relayout_if_dirty();
        let _ = self.tree.commands();
    }

    fn holding(&mut self, modifiers: telar::ModifiersState, act: impl FnOnce(&mut Self)) {
        self.send(Event::ModifiersChanged { modifiers });
        act(self);
        self.send(Event::ModifiersChanged {
            modifiers: Default::default(),
        });
    }

    fn click(&mut self, name: &str) {
        let rect = ui_core::accessibility::snapshot(&self.tree.commands())
            .into_iter()
            .find(|node| node.role == Role::Option && node.name == name)
            .unwrap_or_else(|| panic!("{name}"))
            .rect;
        let (x, y) = telar::testing::centre(rect);
        self.send(press(x, y));
        self.send(release(x, y));
    }

    fn key(&mut self, key: Key, modifiers: telar::ModifiersState) {
        self.holding(modifiers, |list| list.send(key_with(key, modifiers)));
    }

    fn chosen(&self) -> Vec<String> {
        self.chosen.get().iter().map(|id| id.to_string()).collect()
    }

    fn options(&self) -> Vec<(String, Option<bool>)> {
        ui_core::accessibility::snapshot(&self.tree.commands())
            .into_iter()
            .filter(|node| node.role == Role::Option)
            .map(|node| (node.name, node.toggled))
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

/// A list that takes several rows says so, and every option says whether it is one of them, the ones left out included.
#[test]
fn a_list_of_several_choices_is_multiselectable_and_each_option_says_if_it_is_chosen() {
    let mut list = Several::new(fruit());
    let read = ui_core::accessibility::snapshot(&list.tree.commands());
    let listbox = read
        .iter()
        .find(|node| node.role == Role::ListBox)
        .expect("a list box");
    assert!(listbox.multiselectable);
    list.click("Apple 3");
    list.holding(ctrl(), |list| list.click("Cherry"));
    assert_eq!(list.chosen(), ["apple", "cherry"]);
    assert_eq!(
        list.options(),
        [
            ("Apple 3".to_string(), Some(true)),
            ("Banana".to_string(), Some(false)),
            ("Blueberry".to_string(), Some(false)),
            ("Cherry".to_string(), Some(true)),
        ]
    );
    assert_eq!(
        *list.heard.borrow(),
        ["change 1", "select apple", "change 2"],
        "a plain press chooses its row on its own; a Ctrl press only adds"
    );
    list.holding(ctrl(), |list| list.click("Apple 3"));
    assert_eq!(list.chosen(), ["cherry"], "and takes away again");
}

#[test]
fn a_shift_press_chooses_the_run_from_the_last_row_pressed_passing_over_disabled_rows() {
    let mut list = Several::new(fruit());
    list.click("Apple 3");
    list.holding(shift(), |list| list.click("Cherry"));
    assert_eq!(list.chosen(), ["apple", "blueberry", "cherry"]);
    list.click("Blueberry");
    assert_eq!(list.chosen(), ["blueberry"], "a plain press starts over");
}

#[test]
fn space_toggles_shift_with_the_arrows_extends_and_ctrl_a_takes_every_row() {
    let mut list = Several::new(fruit());
    let id = telar::focus::exposed()
        .into_iter()
        .find(|exposed| exposed.role == Role::ListBox)
        .expect("a list box")
        .id;
    telar::focus::request(id);
    relayout_if_dirty();
    list.key(Key::Named(NamedKey::Space), plain());
    assert_eq!(list.chosen(), ["apple"]);
    list.key(Key::Named(NamedKey::ArrowDown), shift());
    list.key(Key::Named(NamedKey::ArrowDown), shift());
    assert_eq!(list.chosen(), ["apple", "blueberry", "cherry"]);
    list.key(Key::Named(NamedKey::ArrowUp), shift());
    assert_eq!(list.chosen(), ["apple", "blueberry"]);
    list.key(Key::Named(NamedKey::Space), plain());
    assert_eq!(list.chosen(), ["apple"], "Space took the cursor's row away");
    list.key(Key::Named(NamedKey::Enter), plain());
    assert_eq!(
        list.chosen(),
        ["blueberry"],
        "Enter chooses the row on its own"
    );
    list.key(Key::Char('a'), ctrl());
    assert_eq!(
        list.chosen(),
        ["apple", "blueberry", "cherry"],
        "every row that can be chosen"
    );
    list.key(Key::Char('a'), ctrl());
    assert!(list.chosen().is_empty(), "and none once all are");
}

#[test]
fn a_list_of_one_choice_is_not_multiselectable_and_ignores_the_modifiers() {
    let mut mounted = Mounted::new(fruit(), 300.0);
    let nodes = mounted.read();
    let listbox = nodes
        .iter()
        .find(|node| node.role == Role::ListBox)
        .expect("a list box");
    assert!(!listbox.multiselectable);
    let rect = |name: &str, mounted: &Mounted| {
        mounted
            .read()
            .into_iter()
            .find(|node| node.role == Role::Option && node.name == name)
            .unwrap_or_else(|| panic!("{name}"))
            .rect
    };
    let (x, y) = telar::testing::centre(rect("Apple 3", &mounted));
    mounted.send(press(x, y));
    mounted.send(release(x, y));
    mounted.send(Event::ModifiersChanged { modifiers: ctrl() });
    let (x, y) = telar::testing::centre(rect("Cherry", &mounted));
    mounted.send(press(x, y));
    mounted.send(release(x, y));
    mounted.send(Event::ModifiersChanged { modifiers: plain() });
    assert_eq!(mounted.selected().as_deref(), Some("cherry"));
    assert_eq!(*mounted.picked.borrow(), ["apple", "cherry"]);
}

/// The words read inside a status.
fn statuses(tree: &ComponentList) -> Vec<String> {
    let nodes = ui_core::accessibility::snapshot(&tree.commands());
    nodes
        .iter()
        .filter(|node| {
            node.parent
                .is_some_and(|parent| nodes[parent].role == Role::Status)
        })
        .map(|node| node.name.clone())
        .collect()
}

/// A list with no rows says so as a status, in the words it was given or the catalogue's own, and stops saying it once rows arrive.
#[test]
fn an_empty_list_says_so_until_its_rows_arrive() {
    crate::test_support::fresh_layout_runtime();
    telar::focus::clear();
    let items = signal(Vec::<ListRow>::new());
    let item = virtual_list(
        VirtualListProps::props()
            .items(Reactive::of(move || items.get()))
            .label("Fruit")
            .build(),
        Children::default(),
    )
    .unwrap();
    lay_out(item.layout_node(), 240.0, 200.0);
    let mut tree = ComponentList::new(item);
    let _ = tree.commands();
    assert_eq!(statuses(&tree), ["Nothing to show"]);
    let list = telar::focus::exposed()
        .into_iter()
        .find(|exposed| exposed.role == Role::ListBox)
        .expect("the list is still there to focus");
    telar::focus::request(list.id);
    route(&mut tree, &named(NamedKey::ArrowDown));
    items.set(fruit());
    relayout_if_dirty();
    let _ = tree.commands();
    assert!(statuses(&tree).is_empty(), "{:?}", statuses(&tree));
    items.set(Vec::new());
    relayout_if_dirty();
    assert_eq!(statuses(&tree), ["Nothing to show"]);
}

#[test]
fn an_empty_list_shows_its_own_words_or_its_children() {
    crate::test_support::fresh_layout_runtime();
    let words = telar::testing::scoped(|| {
        let words = virtual_list(
            VirtualListProps::props()
                .empty_label("No fruit yet")
                .build(),
            Children::default(),
        )
        .unwrap();
        lay_out(words.layout_node(), 240.0, 200.0);
        ComponentList::new(words)
    });
    assert_eq!(statuses(&words), ["No fruit yet"]);
    drop(words);

    crate::test_support::fresh_layout_runtime();
    let built = telar::testing::scoped(|| {
        let built = virtual_list(
            VirtualListProps::props().empty_label("Unused").build(),
            Children::new(|| {
                let mut slots = Slots::new();
                let text =
                    Text::declaring(|| "Add a fruit".to_string(), LayoutStyle::new(), |t| t)?;
                slots.push(None, box_item(text));
                Ok(slots)
            }),
        )
        .unwrap();
        lay_out(built.layout_node(), 240.0, 200.0);
        ComponentList::new(built)
    });
    assert_eq!(statuses(&built), ["Add a fruit"]);
}

fn wordy(count: usize) -> Vec<ListRow> {
    (0..count)
        .map(|i| {
            let label = match i % 3 {
                0 => format!("Row {i:03}"),
                _ => format!("Row {i:03} has a label long enough to run onto more lines than one"),
            };
            ListRow::new(format!("row-{i}"), label)
        })
        .collect()
}

fn long(count: usize) -> Vec<ListRow> {
    (0..count)
        .map(|i| {
            ListRow::new(
                format!("row-{i}"),
                format!("Row {i:03} has a label long enough to run onto a second line"),
            )
        })
        .collect()
}

/// The list's view and the options built in it, by name, in window space.
fn laid_options(tree: &ComponentList) -> (telar::Rect, Vec<(String, telar::Rect)>) {
    let nodes = ui_core::accessibility::snapshot(&tree.commands());
    let view = nodes
        .iter()
        .find(|node| node.role == Role::ListBox)
        .expect("a list box")
        .rect;
    let options = nodes
        .into_iter()
        .filter(|node| node.role == Role::Option)
        .map(|node| (node.name, node.rect))
        .collect();
    (view, options)
}

impl Mounted {
    fn cursor_rect(&self) -> telar::Rect {
        let name = self.pointed().expect("the cursor's row is built");
        let (_, options) = laid_options(&self.tree);
        options
            .into_iter()
            .find(|(option, _)| *option == name)
            .map(|(_, rect)| rect)
            .unwrap_or_else(|| panic!("`{name}` is drawn"))
    }

    fn view(&self) -> telar::Rect {
        laid_options(&self.tree).0
    }
}

fn assert_wholly_in(view: telar::Rect, row: telar::Rect, what: &str) {
    assert!(
        row.y >= view.y - 0.5 && row.y + row.height <= view.y + view.height + 0.5,
        "{what}: the row at {}..{} is not wholly in the view at {}..{}",
        row.y,
        row.y + row.height,
        view.y,
        view.y + view.height
    );
}

#[test]
fn a_wrapping_list_grows_a_row_to_hold_its_label_and_leaves_a_short_one_a_row_tall() {
    let mounted = Mounted::wrapping(wordy(12), 400.0);
    let (_, options) = laid_options(&mounted.tree);
    let height = |name: &str| {
        options
            .iter()
            .find(|(option, _)| option.starts_with(name))
            .map(|(_, rect)| rect.height)
            .unwrap_or_else(|| panic!("`{name}` is drawn among {options:?}"))
    };
    let row_height = ListTokens::current().row_height();
    assert_eq!(height("Row 000"), row_height, "a one-line row");
    assert!(
        height("Row 001") > row_height * 1.5,
        "a wrapped row is taller"
    );
    for pair in options.windows(2) {
        let (above, below) = (pair[0].1, pair[1].1);
        assert!(
            (above.y + above.height - below.y).abs() < 0.5,
            "each row starts where the one above it ends: {pair:?}"
        );
    }
}

#[test]
fn a_row_builder_s_lines_are_given_the_height_they_ask_for_in_a_wrapping_list() {
    crate::test_support::fresh_layout_runtime();
    let item = virtual_list(
        VirtualListProps::props()
            .items(fruit())
            .wrap(true)
            .row(Rc::new(|row: &ListRow| {
                let (name, detail) = (row.label.clone(), format!("{} in stock", row.id));
                let lines = vec![
                    box_item(Text::declaring(
                        move || name.to_string(),
                        LayoutStyle::new(),
                        |t| t,
                    )?),
                    box_item(Text::declaring(
                        move || detail.clone(),
                        LayoutStyle::new(),
                        |t| t,
                    )?),
                ];
                Ok(box_item(telar::Container::new(
                    LayoutStyle::column(),
                    lines,
                )?))
            }))
            .build(),
        Children::default(),
    )
    .unwrap();
    lay_out(item.layout_node(), 240.0, 300.0);
    let tree = ComponentList::new(item);
    for _ in 0..8 {
        relayout_if_dirty();
        let _ = tree.commands();
    }
    let (_, options) = laid_options(&tree);
    assert_eq!(options.len(), 4);
    let row_height = ListTokens::current().row_height();
    assert!(
        options.iter().all(|(_, rect)| rect.height > row_height),
        "two lines and the space round them: {options:?}"
    );
}

#[test]
fn a_wrapping_list_brings_the_cursor_s_row_wholly_into_view_however_tall() {
    let mut mounted = Mounted::wrapping(wordy(200), 200.0);
    let list = telar::focus::exposed()
        .into_iter()
        .find(|exposed| exposed.role == Role::ListBox)
        .expect("a list box");
    telar::focus::request(list.id);
    mounted.settle();
    for step in 0..30 {
        mounted.key(NamedKey::ArrowDown);
        mounted.settle();
        assert_wholly_in(
            mounted.view(),
            mounted.cursor_rect(),
            &format!("down {step}"),
        );
    }
    mounted.key(NamedKey::End);
    mounted.settle();
    assert_eq!(
        mounted.pointed().as_deref(),
        Some(wordy(200)[199].label.as_ref())
    );
    let (view, last) = (mounted.view(), mounted.cursor_rect());
    assert_wholly_in(view, last, "the last row");
    assert!(
        (last.y + last.height - (view.y + view.height)).abs() < 0.5,
        "brought in from above, it stops at the bottom edge"
    );
    for step in 0..10 {
        mounted.key(NamedKey::ArrowUp);
        mounted.settle();
        assert_wholly_in(mounted.view(), mounted.cursor_rect(), &format!("up {step}"));
    }
    mounted.key(NamedKey::Home);
    mounted.settle();
    assert!((mounted.cursor_rect().y - mounted.view().y).abs() < 0.5);
}

#[test]
fn page_down_and_up_move_a_wrapping_list_by_the_rows_its_view_shows_whole() {
    let mut mounted = Mounted::wrapping(long(100), 200.0);
    let list = telar::focus::exposed()
        .into_iter()
        .find(|exposed| exposed.role == Role::ListBox)
        .expect("a list box");
    telar::focus::request(list.id);
    mounted.settle();
    mounted.key(NamedKey::Home);
    mounted.settle();
    let tall = mounted.cursor_rect().height;
    assert!(tall > ListTokens::current().row_height(), "the rows wrap");
    let page = (mounted.view().height / tall).floor() as usize;
    let at = |i: usize| long(100)[i].label.to_string();
    mounted.key(NamedKey::PageDown);
    mounted.settle();
    assert_eq!(mounted.pointed(), Some(at(page)));
    assert_wholly_in(mounted.view(), mounted.cursor_rect(), "a page down");
    mounted.key(NamedKey::PageDown);
    mounted.settle();
    assert_eq!(mounted.pointed(), Some(at(page * 2)));
    mounted.key(NamedKey::PageUp);
    mounted.settle();
    assert_eq!(mounted.pointed(), Some(at(page)));
    assert_wholly_in(mounted.view(), mounted.cursor_rect(), "a page back up");
}

#[test]
fn a_wrapping_list_s_options_say_their_place_among_all_the_rows() {
    let mut mounted = Mounted::wrapping(wordy(200), 120.0);
    let list = telar::focus::exposed()
        .into_iter()
        .find(|exposed| exposed.role == Role::ListBox)
        .expect("a list box");
    telar::focus::request(list.id);
    mounted.key(NamedKey::End);
    mounted.settle();
    let places: Vec<Option<SetPosition>> = mounted
        .read()
        .into_iter()
        .filter(|node| node.role == Role::Option)
        .map(|node| node.position)
        .collect();
    assert!(places.len() < 200, "only a screenful is built");
    assert_eq!(
        places.last(),
        Some(&Some(SetPosition {
            level: 1,
            position: 200,
            size: 200,
        }))
    );
}

#[test]
fn shift_with_page_down_chooses_a_page_of_wrapped_rows() {
    let mut list = Several::mount(long(50), true);
    let id = telar::focus::exposed()
        .into_iter()
        .find(|exposed| exposed.role == Role::ListBox)
        .expect("a list box")
        .id;
    telar::focus::request(id);
    for _ in 0..8 {
        relayout_if_dirty();
        let _ = list.tree.commands();
    }
    list.key(Key::Named(NamedKey::Space), plain());
    assert_eq!(list.chosen(), ["row-0"]);
    list.key(Key::Named(NamedKey::PageDown), shift());
    let chosen = list.chosen().len();
    assert!(
        chosen > 1 && chosen < 50,
        "the anchor's row through the page's: {chosen} chosen"
    );
}

/// A reader steps into the list box to find its options, each with the words it draws, in the order they show.
#[test]
fn the_list_box_holds_its_options() {
    let mounted = Mounted::new(fruit(), 400.0);
    let nodes = ui_core::accessibility::snapshot(&mounted.tree.commands());
    let list = nodes
        .iter()
        .position(|node| node.role == Role::ListBox)
        .expect("the list is read as a list box");
    let options: Vec<&str> = nodes
        .iter()
        .filter(|node| node.parent == Some(list))
        .map(|node| {
            assert_eq!(node.role, Role::Option, "{node:?}");
            node.name.as_str()
        })
        .collect();
    assert_eq!(options, ["Apple 3", "Banana", "Blueberry", "Cherry"]);
}
