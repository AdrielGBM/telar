use telar::testing::{key_with, lay_out, named, press, release, route};
use telar::{AccessNode, ComponentList, DrawCommand, Event, relayout_if_dirty};

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
