use std::cell::RefCell;

use telar::testing::{key_with, lay_out, named, press, release, route};
use telar::{ComponentList, DrawCommand, Event, relayout_if_dirty};

use super::*;

fn catalogue() -> Vec<TreeNode> {
    vec![
        TreeNode::new("inputs", "Inputs").with_children([
            TreeNode::new("button", "Button").with_children([
                TreeNode::new("button/primary", "Primary"),
                TreeNode::new("button/ghost", "Ghost"),
            ]),
            TreeNode::new("checkbox", "Checkbox").with_badge("new"),
        ]),
        TreeNode::new("layout", "Layout").with_children([TreeNode::new("stack", "Stack")]),
        TreeNode::new("about", "About"),
    ]
}

struct Mounted {
    tree: ComponentList,
    selected: RwSignal<Option<Arc<str>>>,
    expanded: RwSignal<HashSet<Arc<str>>>,
    picked: Rc<RefCell<Vec<String>>>,
}

impl Mounted {
    fn new(items: Vec<TreeNode>, height: f32) -> Self {
        Self::build(items, height, true)
    }

    fn build(items: Vec<TreeNode>, height: f32, select_branches: bool) -> Self {
        crate::test_support::fresh_layout_runtime();
        ui_core::reset_keyboard();
        focus::clear();
        let selected = signal(None);
        let expanded = signal(HashSet::new());
        let picked = Rc::new(RefCell::new(Vec::new()));
        let sink = picked.clone();
        let item = tree_view(
            TreeViewProps::props()
                .items(items)
                .selected(selected)
                .expanded(expanded)
                .on_select(Rc::new(move |node: &TreeNode| {
                    sink.borrow_mut().push(node.id.to_string())
                }))
                .label("Components")
                .select_branches(select_branches)
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
            expanded,
            picked,
        }
    }

    fn focused(items: Vec<TreeNode>) -> Self {
        let mounted = Self::new(items, 400.0);
        focus_newest_tree();
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

    fn keys(&mut self, keys: &[NamedKey]) {
        for key in keys {
            self.key(key.clone());
        }
    }

    fn type_char(&mut self, c: char) {
        self.send(key_with(Key::Char(c), Default::default()));
    }

    fn click(&mut self, x: f64, y: f64) {
        self.send(press(x, y));
        self.send(release(x, y));
    }

    fn selected(&self) -> Option<String> {
        self.selected.get().as_deref().map(str::to_string)
    }

    fn open(&self) -> Vec<String> {
        let mut open: Vec<String> = self
            .expanded
            .get()
            .iter()
            .map(|id| id.to_string())
            .collect();
        open.sort();
        open
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
}

fn ids(flat: &Flat) -> Vec<&str> {
    flat.0.iter().map(|row| &*row.node.id).collect()
}

#[test]
fn the_open_branches_decide_which_rows_show_and_where_each_sits() {
    let shut = Flat::of(&catalogue(), &HashSet::new());
    assert_eq!(ids(&shut), ["inputs", "layout", "about"]);

    let open: HashSet<Arc<str>> = ["inputs".into(), "button".into()].into_iter().collect();
    let flat = Flat::of(&catalogue(), &open);
    assert_eq!(
        ids(&flat),
        [
            "inputs",
            "button",
            "button/primary",
            "button/ghost",
            "checkbox",
            "layout",
            "about"
        ]
    );
    let place = |i: usize| flat.0[i].position;
    assert_eq!(
        place(1),
        SetPosition {
            level: 2,
            position: 1,
            size: 2
        }
    );
    assert_eq!(
        place(3),
        SetPosition {
            level: 3,
            position: 2,
            size: 2
        }
    );
    assert_eq!(
        place(5),
        SetPosition {
            level: 1,
            position: 2,
            size: 3
        }
    );
    assert_eq!(
        flat.0[4].parent,
        Some(0),
        "a row knows the row it sits under"
    );
    assert_eq!(flat.0[3].parent, Some(1));
    assert_eq!(flat.0[6].parent, None);
}

#[test]
fn the_arrows_walk_the_rows_and_enter_or_space_selects() {
    let mut tree = Mounted::focused(catalogue());
    tree.keys(&[NamedKey::ArrowDown, NamedKey::ArrowDown, NamedKey::Enter]);
    assert_eq!(tree.selected().as_deref(), Some("about"));

    tree.keys(&[NamedKey::ArrowDown, NamedKey::Enter]);
    assert_eq!(
        tree.selected().as_deref(),
        Some("about"),
        "the last row is as far as the cursor goes"
    );

    tree.keys(&[NamedKey::ArrowUp, NamedKey::Space]);
    assert_eq!(tree.selected().as_deref(), Some("layout"));

    tree.keys(&[NamedKey::Home, NamedKey::Enter]);
    assert_eq!(tree.selected().as_deref(), Some("inputs"));

    tree.keys(&[NamedKey::End, NamedKey::Enter]);
    assert_eq!(tree.selected().as_deref(), Some("about"));
    assert_eq!(
        *tree.picked.borrow(),
        ["about", "about", "layout", "inputs", "about"],
        "every commit reaches on_select"
    );
}

#[test]
fn walking_the_rows_commits_nothing() {
    let mut tree = Mounted::focused(catalogue());
    tree.keys(&[NamedKey::ArrowDown, NamedKey::ArrowDown, NamedKey::Home]);
    assert_eq!(tree.selected(), None);
    assert!(tree.picked.borrow().is_empty());
}

#[test]
fn an_unfocused_tree_hears_no_keys() {
    let mut tree = Mounted::new(catalogue(), 400.0);
    tree.keys(&[NamedKey::ArrowRight, NamedKey::Enter]);
    assert_eq!(tree.selected(), None);
    assert!(tree.open().is_empty());
}

#[test]
fn right_opens_then_enters_and_left_climbs_then_shuts() {
    let mut tree = Mounted::focused(catalogue());

    tree.key(NamedKey::ArrowRight);
    assert_eq!(tree.open(), ["inputs"], "right on a shut branch opens it");
    tree.key(NamedKey::Enter);
    assert_eq!(
        tree.selected().as_deref(),
        Some("inputs"),
        "and leaves the cursor on it"
    );

    tree.keys(&[NamedKey::ArrowRight, NamedKey::Enter]);
    assert_eq!(
        tree.selected().as_deref(),
        Some("button"),
        "right on an open branch goes to its first child"
    );

    tree.keys(&[NamedKey::ArrowRight, NamedKey::ArrowRight, NamedKey::Enter]);
    assert_eq!(tree.open(), ["button", "inputs"]);
    assert_eq!(tree.selected().as_deref(), Some("button/primary"));

    tree.keys(&[NamedKey::ArrowRight, NamedKey::Enter]);
    assert_eq!(
        tree.selected().as_deref(),
        Some("button/primary"),
        "right on a leaf does nothing"
    );

    tree.keys(&[NamedKey::ArrowLeft, NamedKey::Enter]);
    assert_eq!(
        tree.selected().as_deref(),
        Some("button"),
        "left on a leaf climbs to its parent"
    );

    tree.key(NamedKey::ArrowLeft);
    assert_eq!(tree.open(), ["inputs"], "left on an open branch shuts it");

    tree.keys(&[NamedKey::ArrowLeft, NamedKey::Enter]);
    assert_eq!(tree.selected().as_deref(), Some("inputs"));
    tree.key(NamedKey::ArrowLeft);
    assert!(tree.open().is_empty());
}

#[test]
fn typing_moves_to_the_visible_row_whose_name_it_starts() {
    let chosen_after = |c: char| {
        let mut tree = Mounted::focused(catalogue());
        tree.type_char(c);
        tree.key(NamedKey::Enter);
        tree.selected()
    };
    assert_eq!(chosen_after('a').as_deref(), Some("about"));
    assert_eq!(
        chosen_after('L').as_deref(),
        Some("layout"),
        "case does not matter"
    );
    assert_eq!(
        chosen_after('p').as_deref(),
        Some("inputs"),
        "a row inside a shut branch is not on screen, so it is not found"
    );
}

#[test]
fn a_space_inside_a_query_is_a_character_not_a_selection() {
    let items = vec![
        TreeNode::new("set-a", "Set A"),
        TreeNode::new("set-b", "Set B"),
        TreeNode::new("settings", "Settings"),
    ];
    let mut tree = Mounted::focused(items);
    for c in "set".chars() {
        tree.type_char(c);
    }
    tree.key(NamedKey::Space);
    assert_eq!(tree.selected(), None, "the space extended the query");
    tree.type_char('b');
    tree.key(NamedKey::Enter);
    assert_eq!(tree.selected().as_deref(), Some("set-b"));
}

#[test]
fn a_bound_selection_and_open_set_are_read_and_written_both_ways() {
    let mut tree = Mounted::new(catalogue(), 400.0);
    tree.expanded.set(["inputs".into()].into_iter().collect());
    tree.selected.set(Some("checkbox".into()));
    relayout_if_dirty();
    focus_newest_tree();
    assert!(
        tree.drawn().iter().any(|text| text == "Checkbox"),
        "opening the bound set shows the children"
    );

    tree.keys(&[NamedKey::ArrowUp, NamedKey::Enter]);
    assert_eq!(
        tree.selected().as_deref(),
        Some("button"),
        "focus arriving at the tree puts the cursor on the bound selection"
    );
}

#[test]
fn clicking_a_row_selects_it_and_clicking_its_caret_opens_it() {
    let mut tree = Mounted::new(catalogue(), 400.0);
    let row = row_height() as f64;
    let caret_x = (pad_x() + caret_width() / 2.0) as f64;

    tree.click(120.0, row * 1.5);
    assert_eq!(tree.selected().as_deref(), Some("layout"));
    assert_eq!(*tree.picked.borrow(), ["layout"]);
    assert_eq!(
        focus::current(),
        focus::exposed()
            .iter()
            .filter(|e| e.role == Role::Tree)
            .map(|e| e.id)
            .max(),
        "a click on a row gives the tree the keyboard"
    );

    tree.click(caret_x, row * 0.5);
    assert_eq!(tree.open(), ["inputs"]);
    assert_eq!(
        tree.selected().as_deref(),
        Some("layout"),
        "the caret opens the branch without choosing it"
    );

    tree.click(caret_x, row * 0.5);
    assert!(tree.open().is_empty(), "and shuts it again");
}

#[test]
fn shutting_a_branch_around_the_cursor_brings_the_cursor_out() {
    let mut tree = Mounted::focused(catalogue());
    tree.keys(&[
        NamedKey::ArrowRight,
        NamedKey::ArrowRight,
        NamedKey::ArrowDown,
    ]);
    let caret_x = (pad_x() + caret_width() / 2.0) as f64;
    tree.click(caret_x, row_height() as f64 * 0.5);
    tree.key(NamedKey::Enter);
    assert_eq!(tree.selected().as_deref(), Some("inputs"));
}

#[test]
fn every_row_says_it_is_a_tree_item_selected_open_and_where_it_sits() {
    let tree = Mounted::focused(catalogue());
    tree.expanded.set(["inputs".into()].into_iter().collect());
    tree.selected.set(Some("button".into()));
    relayout_if_dirty();
    let _ = tree.tree.commands();

    let exposed = focus::exposed();
    assert_eq!(
        exposed.iter().filter(|e| e.role == Role::Tree).count(),
        1,
        "the tree is one control"
    );
    let rows: Vec<(Option<bool>, Option<bool>, Option<SetPosition>)> = exposed
        .iter()
        .filter(|e| e.role == Role::TreeItem)
        .map(|e| (e.toggled, e.expanded, e.position))
        .collect();
    let place = |level, position, size| {
        Some(SetPosition {
            level,
            position,
            size,
        })
    };
    for expected in [
        (Some(false), Some(true), place(1, 1, 3)),
        (Some(true), Some(false), place(2, 1, 2)),
        (Some(false), None, place(2, 2, 2)),
        (Some(false), Some(false), place(1, 2, 3)),
        (Some(false), None, place(1, 3, 3)),
    ] {
        assert!(rows.contains(&expected), "{expected:?} is among {rows:?}");
    }
    assert_eq!(rows.len(), 5, "one entry per visible row: {rows:?}");
}

fn forest(groups: usize, leaves: usize) -> Vec<TreeNode> {
    (0..groups)
        .map(|g| {
            TreeNode::new(format!("group {g}"), format!("Group {g}")).with_children(
                (0..leaves).map(|l| TreeNode::new(format!("{g}/{l}"), format!("Node {g}/{l}"))),
            )
        })
        .collect()
}

#[test]
fn only_the_rows_on_screen_are_built_however_large_the_tree() {
    let height = row_height() * 10.0;
    let mut tree = Mounted::new(forest(100, 100), height);
    tree.expanded
        .set((0..100).map(|g| Arc::from(format!("group {g}"))).collect());
    relayout_if_dirty();

    let built = |tree: &Mounted| {
        tree.drawn()
            .into_iter()
            .filter(|text| text.starts_with("Node") || text.starts_with("Group"))
            .collect::<Vec<_>>()
    };
    let first = built(&tree);
    assert!(first.contains(&"Group 0".to_string()), "{first:?}");
    assert!(
        first.len() <= 10 + 1 + 2 * OVERSCAN,
        "10,100 rows open, and only a screenful plus the overscan is built: {}",
        first.len()
    );
    assert!(
        focus::exposed()
            .iter()
            .filter(|e| e.role == Role::TreeItem)
            .count()
            <= 10 + 1 + 2 * OVERSCAN,
        "and only those are announced"
    );

    focus_newest_tree();
    tree.key(NamedKey::End);
    let last = built(&tree);
    assert!(
        last.contains(&"Node 99/99".to_string()),
        "End scrolls the last row into view and builds it: {last:?}"
    );
    assert!(!last.contains(&"Group 0".to_string()));
    tree.key(NamedKey::Enter);
    assert_eq!(tree.selected().as_deref(), Some("99/99"));
}

/// Tab onto the tree this test built, which need not be the first in the order: a widget a test is done with stays registered.
fn focus_newest_tree() {
    let tree = focus::exposed()
        .into_iter()
        .filter(|exposed| exposed.role == Role::Tree)
        .map(|exposed| exposed.id)
        .max()
        .expect("a tree is on screen");
    focus::request(tree);
}

#[test]
fn right_to_left_swaps_which_arrow_opens_and_which_shuts() {
    let mut tree = Mounted::focused(catalogue());
    telar::set_direction(Direction::Rtl);
    tree.key(NamedKey::ArrowLeft);
    assert_eq!(
        tree.open(),
        ["inputs"],
        "left opens when the tree reads right to left"
    );
    tree.key(NamedKey::ArrowRight);
    telar::set_direction(Direction::Ltr);
    assert!(tree.open().is_empty(), "and right shuts");
}

/// The name of the row a focused tree points a reader at, or `None` when it points at none.
fn pointed(tree: &Mounted) -> Option<String> {
    let nodes = ui_core::accessibility::snapshot(&tree.tree.commands());
    let focused = nodes
        .iter()
        .find(|node| node.role == Role::Tree && node.focused)?;
    let row = focused.active_descendant?;
    nodes
        .iter()
        .find(|node| node.id == Some(row))
        .map(|node| node.name.clone())
}

#[test]
fn a_reader_is_pointed_at_the_row_the_cursor_is_on() {
    let mut tree = Mounted::focused(catalogue());
    assert_eq!(pointed(&tree).as_deref(), Some("Inputs"));
    tree.key(NamedKey::ArrowDown);
    assert_eq!(pointed(&tree).as_deref(), Some("Layout"));
    tree.key(NamedKey::ArrowLeft);
    tree.key(NamedKey::ArrowRight);
    assert_eq!(
        pointed(&tree).as_deref(),
        Some("Layout"),
        "opening a row keeps the reader on it"
    );
    tree.key(NamedKey::ArrowRight);
    assert_eq!(pointed(&tree).as_deref(), Some("Stack"));
}

#[test]
fn a_row_scrolled_away_and_back_is_pointed_at_again() {
    let height = row_height() * 10.0;
    let mut tree = Mounted::new(forest(100, 100), height);
    tree.expanded
        .set((0..100).map(|g| Arc::from(format!("group {g}"))).collect());
    relayout_if_dirty();
    focus_newest_tree();
    let _ = tree.tree.commands();

    assert_eq!(pointed(&tree).as_deref(), Some("Group 0"));
    tree.key(NamedKey::End);
    assert_eq!(pointed(&tree).as_deref(), Some("Node 99/99"));
    tree.key(NamedKey::Home);
    assert_eq!(
        pointed(&tree).as_deref(),
        Some("Group 0"),
        "the first row, built again, is the one pointed at"
    );
}

#[test]
fn a_tree_that_selects_only_leaves_opens_a_branch_it_is_asked_to_choose() {
    let mut tree = Mounted::build(catalogue(), 400.0, false);
    focus_newest_tree();
    tree.key(NamedKey::Enter);
    assert_eq!(tree.open(), ["inputs"]);
    assert_eq!(tree.selected(), None, "a branch is opened, not chosen");

    tree.keys(&[NamedKey::ArrowDown, NamedKey::ArrowDown, NamedKey::Space]);
    assert_eq!(tree.selected().as_deref(), Some("checkbox"));

    let row = row_height() as f64;
    tree.click(120.0, row * 0.5);
    assert!(tree.open().is_empty(), "a click on a branch shuts it");
    assert_eq!(tree.selected().as_deref(), Some("checkbox"));
    assert_eq!(*tree.picked.borrow(), ["checkbox"]);
}
