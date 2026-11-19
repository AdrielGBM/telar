use std::cell::RefCell;
use std::num::NonZeroU32;

use std::time::Duration;

use telar::testing::{advance_time, key_with, lay_out, moved, named, press, release, route};
use telar::{
    ComponentList, DragItem, DragSession, DrawCommand, Event, Rect, relayout_if_dirty,
};

use super::super::tree_drag::{TreeDrag, TreeTarget};

use super::*;

fn row_height() -> f32 {
    TreeViewTokens::current().row_height(ControlSize::Regular)
}

fn pad_x() -> f32 {
    TreeViewTokens::current().pad_x(ControlSize::Regular)
}

fn inset() -> f32 {
    TreeViewTokens::current().inset(ControlSize::Regular)
}

fn caret_width() -> f32 {
    crate::shared::icon_size(telar::ControlSize::Regular)
}

type Node = TreeNode<Arc<str>>;
type Builder = TreeViewPropsBuilder<Arc<str>>;

fn node(id: impl Into<Arc<str>>, label: impl Into<Arc<str>>) -> Node {
    TreeNode::new(id.into(), label)
}

fn catalogue() -> Vec<Node> {
    vec![
        node("inputs", "Inputs").with_children([
            node("button", "Button").with_children([
                node("button/primary", "Primary"),
                node("button/ghost", "Ghost"),
            ]),
            node("checkbox", "Checkbox").with_badge("new"),
        ]),
        node("layout", "Layout").with_children([node("stack", "Stack")]),
        node("about", "About"),
    ]
}

struct Mounted {
    tree: ComponentList,
    selected: RwSignal<Option<Arc<str>>>,
    expanded: RwSignal<HashSet<Arc<str>>>,
    picked: Rc<RefCell<Vec<String>>>,
    asked: Rc<RefCell<Vec<TreeContext<Arc<str>>>>>,
}

impl Mounted {
    fn new(items: Vec<Node>, height: f32) -> Self {
        Self::build(items, height, true)
    }

    fn build(items: Vec<Node>, height: f32, select_branches: bool) -> Self {
        Self::mount(items, height, select_branches, false)
    }

    fn wrapping(items: Vec<Node>, height: f32) -> Self {
        let mut mounted = Self::mount(items, height, true, true);
        mounted.settle();
        mounted
    }

    fn mount(items: Vec<Node>, height: f32, select_branches: bool, wrap: bool) -> Self {
        Self::shaped(items, height, |props| props.select_branches(select_branches).wrap(wrap))
    }

    fn shaped(items: Vec<Node>, height: f32, shape: impl FnOnce(Builder) -> Builder) -> Self {
        crate::test_support::fresh_layout_runtime();
        telar::testing::reset_keyboard();
        focus::clear();
        let selected = signal(None);
        let expanded = signal(HashSet::new());
        let picked = Rc::new(RefCell::new(Vec::new()));
        let sink = picked.clone();
        let asked = Rc::new(RefCell::new(Vec::new()));
        let asking = asked.clone();
        let item = tree_view(shape(
            TreeViewProps::props()
                .items(items)
                .selected(selected)
                .expanded(expanded)
                .on_select(move |node| sink.borrow_mut().push(node.id.to_string()))
                .on_context(move |context| asking.borrow_mut().push(context))
                .label("Components"),
        ).build())
        .unwrap();
        lay_out(item.layout_node(), 240.0, height);
        let tree = ComponentList::new(item);
        let _ = tree.commands();
        Self {
            tree,
            selected,
            expanded,
            picked,
            asked,
        }
    }

    fn focused(items: Vec<Node>) -> Self {
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

    fn settle(&mut self) {
        for _ in 0..8 {
            relayout_if_dirty();
            let _ = self.tree.commands();
        }
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

fn ids(flat: &Flat<Arc<str>>) -> Vec<&str> {
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
            level: NonZeroU32::new(2).unwrap(),
            position: NonZeroU32::MIN,
            size: NonZeroU32::new(2).unwrap()
        }
    );
    assert_eq!(
        place(3),
        SetPosition {
            level: NonZeroU32::new(3).unwrap(),
            position: NonZeroU32::new(2).unwrap(),
            size: NonZeroU32::new(2).unwrap()
        }
    );
    assert_eq!(
        place(5),
        SetPosition {
            level: NonZeroU32::MIN,
            position: NonZeroU32::new(2).unwrap(),
            size: NonZeroU32::new(3).unwrap()
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
        node("set-a", "Set A"),
        node("set-b", "Set B"),
        node("settings", "Settings"),
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
    let caret_x = (inset() + pad_x() + caret_width() / 2.0) as f64;

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

/// The caret is drawn an icon wide where it was, its label beside it where it was, and is found by the pointer across at least the smallest target round what it draws at every size.
#[test]
fn the_caret_is_pressed_across_the_smallest_target_at_every_size() {
    for size in [ControlSize::Small, ControlSize::Regular, ControlSize::Large] {
        crate::test_support::fresh_layout_runtime();
        focus::clear();
        let expanded = signal(HashSet::new());
        let item = tree_view(
            TreeViewProps::props()
                .items(catalogue())
                .expanded(expanded)
                .label("Components")
                .size(size)
                .build(),
        )
        .unwrap();
        lay_out(item.layout_node(), 240.0, 400.0);
        let mut tree = ComponentList::new(item);
        let _ = tree.commands();
        let tokens = TreeViewTokens::current();
        let start = tokens.inset(size) + tokens.pad_x(size);
        let side = crate::shared::icon_size(size);
        let middle = tokens.row_height(size) / 2.0;
        let mut toggled_at = Vec::new();
        for x in 0..60 {
            let was = expanded.get().contains("inputs");
            for event in [
                press(x as f64, middle as f64),
                release(x as f64, middle as f64),
            ] {
                route(&mut tree, &event);
                relayout_if_dirty();
                let _ = tree.commands();
            }
            if expanded.get().contains("inputs") != was {
                toggled_at.push(x as f32);
            }
        }
        let (first, last) = (toggled_at[0], toggled_at[toggled_at.len() - 1]);
        assert_eq!(
            toggled_at.len() as f32,
            last - first + 1.0,
            "{size:?}: one span toggles, {toggled_at:?}"
        );
        assert!(
            last + 1.0 - first >= crate::metrics::MIN_TARGET,
            "{size:?}: the caret is found across {first}..={last}"
        );
        assert!(
            first <= start && start + side <= last + 1.0,
            "{size:?}: {first}..={last} holds the caret drawn at {start}..{}",
            start + side
        );

        let mut label = None;
        telar::for_each_with_matrix(&tree.commands(), |command, matrix| {
            if let DrawCommand::Text { text, rect, .. } = command
                && text.as_ref() == "Inputs"
            {
                label.get_or_insert(telar::transform_clip_rect(matrix, *rect));
            }
        });
        let label = label.expect("the label is drawn");
        let want = start + side + crate::shared::gap_sm(size);
        assert!(
            (label.x - want).abs() < 0.5,
            "{size:?}: the label stands at {}, want {want}",
            label.x
        );
    }
}

#[test]
fn shutting_a_branch_shuts_the_branches_beneath_it() {
    let mut tree = Mounted::focused(catalogue());
    tree.keys(&[
        NamedKey::ArrowRight,
        NamedKey::ArrowRight,
        NamedKey::ArrowRight,
    ]);
    assert_eq!(tree.open(), ["button", "inputs"]);

    tree.keys(&[NamedKey::ArrowUp, NamedKey::ArrowLeft]);
    assert!(tree.open().is_empty(), "by key");

    tree.keys(&[
        NamedKey::ArrowRight,
        NamedKey::ArrowRight,
        NamedKey::ArrowRight,
    ]);
    let caret_x = (inset() + pad_x() + caret_width() / 2.0) as f64;
    tree.click(caret_x, row_height() as f64 * 0.5);
    assert!(tree.open().is_empty(), "by its caret");

    tree.key(NamedKey::ArrowRight);
    assert_eq!(
        tree.open(),
        ["inputs"],
        "opening it again shows its children shut"
    );
}

#[test]
fn shutting_a_branch_around_the_cursor_brings_the_cursor_out() {
    let mut tree = Mounted::focused(catalogue());
    tree.keys(&[
        NamedKey::ArrowRight,
        NamedKey::ArrowRight,
        NamedKey::ArrowDown,
    ]);
    let caret_x = (inset() + pad_x() + caret_width() / 2.0) as f64;
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
    let rows: Vec<(Option<telar::Toggle>, Option<bool>, Option<SetPosition>)> = exposed
        .iter()
        .filter(|e| e.role == Role::TreeItem)
        .map(|e| (e.toggled, e.expanded, e.position))
        .collect();
    let place = |level, position, size| {
        Some(SetPosition {
            level: NonZeroU32::new(level).unwrap(),
            position: NonZeroU32::new(position).unwrap(),
            size: NonZeroU32::new(size).unwrap(),
        })
    };
    for expected in [
        (Some(telar::Toggle::Off), Some(true), place(1, 1, 3)),
        (Some(telar::Toggle::On), Some(false), place(2, 1, 2)),
        (Some(telar::Toggle::Off), None, place(2, 2, 2)),
        (Some(telar::Toggle::Off), Some(false), place(1, 2, 3)),
        (Some(telar::Toggle::Off), None, place(1, 3, 3)),
    ] {
        assert!(rows.contains(&expected), "{expected:?} is among {rows:?}");
    }
    assert_eq!(rows.len(), 5, "one entry per visible row: {rows:?}");
}

fn forest(groups: usize, leaves: usize) -> Vec<Node> {
    (0..groups)
        .map(|g| {
            node(format!("group {g}"), format!("Group {g}")).with_children(
                (0..leaves).map(|l| node(format!("{g}/{l}"), format!("Node {g}/{l}"))),
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
    let nodes = telar::inspect::snapshot(&tree.tree.commands());
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

#[test]
fn the_tree_is_a_tab_stop_and_its_rows_are_not() {
    let _tree = Mounted::new(catalogue(), 400.0);
    focus::clear();
    focus::focus_next();
    let held = focus::current().expect("tab reaches the tree");
    let exposed = focus::exposed();
    assert!(exposed.iter().any(|e| e.id == held && e.role == Role::Tree));
    focus::focus_next();
    assert_eq!(focus::current(), Some(held), "and nothing else is a stop");
}

#[test]
fn pressing_the_empty_space_below_the_rows_focuses_the_tree() {
    let mut tree = Mounted::new(catalogue(), 400.0);
    focus::clear();
    assert_eq!(focus::current(), None);
    tree.click(120.0, 380.0);
    assert!(focus::current().is_some());
}

/// A tree's rows stand at the height every other list's rows do, rather than at a proportion of their own.
#[test]
fn a_tree_s_rows_are_as_tall_as_every_other_list_s() {
    let tree = Mounted::new(catalogue(), 400.0);
    let rows = crate::test_support::drawn(Role::TreeItem);
    assert!(!rows.is_empty());
    assert!(
        rows.iter().all(|row| row.height
            == crate::layout::list::list::ListTokens::default().row_height(ControlSize::Regular)),
        "{rows:?}"
    );
    drop(tree);
}

fn wordy(count: usize) -> Vec<Node> {
    (0..count)
        .map(|i| {
            let label = match i % 3 {
                0 => format!("Node {i:03}"),
                _ => format!("Node {i:03} has a name long enough to run onto more lines than one"),
            };
            node(format!("n{i}"), label)
        })
        .collect()
}

fn long(count: usize) -> Vec<Node> {
    (0..count)
        .map(|i| {
            node(
                format!("n{i}"),
                format!("Node {i:03} has a name long enough to run onto a second line"),
            )
        })
        .collect()
}

/// The tree's view and the rows built in it, by name, in window space.
fn laid_rows(tree: &Mounted) -> (Rect, Vec<(String, Rect)>) {
    let nodes = telar::inspect::snapshot(&tree.tree.commands());
    let view = nodes
        .iter()
        .find(|node| node.role == Role::Tree)
        .expect("a tree")
        .rect;
    let rows = nodes
        .into_iter()
        .filter(|node| node.role == Role::TreeItem)
        .map(|node| (node.name, node.rect))
        .collect();
    (view, rows)
}

fn cursor_rect(tree: &Mounted) -> Rect {
    let name = pointed(tree).expect("the cursor's row is built");
    let (_, rows) = laid_rows(tree);
    rows.into_iter()
        .find(|(row, _)| *row == name)
        .map(|(_, rect)| rect)
        .unwrap_or_else(|| panic!("`{name}` is drawn"))
}

fn assert_wholly_in(view: Rect, row: Rect, what: &str) {
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
fn a_wrapping_tree_grows_a_row_to_hold_its_label_and_leaves_a_short_one_a_row_tall() {
    let tree = Mounted::wrapping(wordy(12), 400.0);
    let (_, rows) = laid_rows(&tree);
    let height = |name: &str| {
        rows.iter()
            .find(|(row, _)| row.starts_with(name))
            .map(|(_, rect)| rect.height)
            .unwrap_or_else(|| panic!("`{name}` is drawn among {rows:?}"))
    };
    assert_eq!(height("Node 000"), row_height(), "a one-line row");
    assert!(
        height("Node 001") > row_height() * 1.5,
        "a wrapped row is taller: {}",
        height("Node 001")
    );
    let ordered: Vec<f32> = rows.iter().map(|(_, rect)| rect.y).collect();
    assert!(
        ordered.windows(2).all(|pair| pair[0] < pair[1]),
        "the rows stack: {ordered:?}"
    );
    for pair in rows.windows(2) {
        let (above, below) = (pair[0].1, pair[1].1);
        assert!(
            (above.y + above.height - below.y).abs() < 0.5,
            "each row starts where the one above it ends: {pair:?}"
        );
    }
}

#[test]
fn a_wrapping_tree_brings_the_cursor_s_row_wholly_into_view_however_tall() {
    let mut tree = Mounted::wrapping(wordy(200), 200.0);
    focus_newest_tree();
    tree.settle();
    for step in 0..30 {
        tree.key(NamedKey::ArrowDown);
        tree.settle();
        let (view, _) = laid_rows(&tree);
        assert_wholly_in(view, cursor_rect(&tree), &format!("down {step}"));
    }
    tree.key(NamedKey::End);
    tree.settle();
    assert_eq!(
        pointed(&tree).as_deref(),
        Some(wordy(200)[199].label.as_ref())
    );
    let (view, _) = laid_rows(&tree);
    let last = cursor_rect(&tree);
    assert_wholly_in(view, last, "the last row");
    assert!(
        (last.y + last.height - (view.y + view.height)).abs() < 0.5,
        "brought in from above, it stops at the bottom edge"
    );
    for step in 0..10 {
        tree.key(NamedKey::ArrowUp);
        tree.settle();
        let (view, _) = laid_rows(&tree);
        assert_wholly_in(view, cursor_rect(&tree), &format!("up {step}"));
    }
    tree.key(NamedKey::Home);
    tree.settle();
    let (view, _) = laid_rows(&tree);
    assert!(
        (cursor_rect(&tree).y - view.y).abs() < 0.5,
        "the first row at the top"
    );
}

#[test]
fn page_down_and_up_move_a_wrapping_tree_by_the_rows_its_view_shows_whole() {
    let mut tree = Mounted::wrapping(long(100), 200.0);
    focus_newest_tree();
    tree.settle();
    tree.key(NamedKey::Home);
    tree.settle();
    let (view, rows) = laid_rows(&tree);
    let tall = rows[0].1.height;
    assert!(tall > row_height(), "the rows wrap: {tall}");
    let page = (view.height / tall).floor() as usize;
    tree.key(NamedKey::PageDown);
    tree.settle();
    let at = |i: usize| long(100)[i].label.to_string();
    assert_eq!(pointed(&tree), Some(at(page)));
    assert_wholly_in(view, cursor_rect(&tree), "a page down");
    tree.key(NamedKey::PageDown);
    tree.settle();
    assert_eq!(pointed(&tree), Some(at(page * 2)));
    assert_wholly_in(view, cursor_rect(&tree), "two pages down");
    tree.key(NamedKey::PageUp);
    tree.settle();
    assert_eq!(pointed(&tree), Some(at(page)));
    assert_wholly_in(view, cursor_rect(&tree), "a page back up");
}

#[test]
fn a_wrapping_tree_s_rows_say_where_they_sit() {
    let mut tree = Mounted::wrapping(long(100), 200.0);
    focus_newest_tree();
    tree.key(NamedKey::End);
    tree.settle();
    let last = focus::exposed()
        .into_iter()
        .filter(|exposed| exposed.role == Role::TreeItem)
        .filter_map(|exposed| exposed.position)
        .max_by_key(|place| place.position);
    assert_eq!(
        last,
        Some(SetPosition {
            level: NonZeroU32::MIN,
            position: NonZeroU32::new(100).unwrap(),
            size: NonZeroU32::new(100).unwrap(),
        })
    );
}

/// A reader steps into the tree to find its rows, every one shown held by the tree itself, in the order they show, each saying its level.
#[test]
fn the_tree_holds_every_row_it_shows() {
    let tree = Mounted::new(catalogue(), 400.0);
    tree.expanded.set(["inputs".into()].into_iter().collect());
    relayout_if_dirty();
    let nodes = telar::inspect::snapshot(&tree.tree.commands());
    let at = nodes
        .iter()
        .position(|node| node.role == Role::Tree)
        .expect("the tree is read as a tree");
    let rows: Vec<(&str, Option<u32>)> = nodes
        .iter()
        .filter(|node| node.parent == Some(at))
        .map(|node| {
            assert_eq!(node.role, Role::TreeItem, "{node:?}");
            (
                node.name.as_str(),
                node.position.map(|place| place.level.get()),
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            ("Inputs", Some(1)),
            ("Button", Some(2)),
            ("Checkbox new", Some(2)),
            ("Layout", Some(1)),
            ("About", Some(1)),
        ]
    );
}

fn secondary(pressed: bool, x: f64, y: f64) -> Event {
    let (button, source) = (PointerButton::Secondary, telar::PointerSource::Mouse);
    match pressed {
        true => Event::PointerPressed {
            x,
            y,
            button,
            source,
        },
        false => Event::PointerReleased {
            x,
            y,
            button,
            source,
        },
    }
}

#[test]
fn a_secondary_press_on_a_row_asks_for_its_menu_at_the_pointer_and_moves_the_cursor_there() {
    let mut tree = Mounted::new(catalogue(), 400.0);
    let (x, y) = (120.0, row_height() as f64 * 1.5);
    tree.send(secondary(true, x, y));
    tree.send(secondary(false, x, y));
    let asked = tree.asked.borrow().clone();
    assert_eq!(asked.len(), 1);
    assert_eq!(&*asked[0].node.id, "layout");
    assert_eq!(asked[0].at, (x as f32, y as f32));
    assert_eq!(asked[0].around, None);
    assert_eq!(pointed(&tree).as_deref(), Some("Layout"));
    assert_eq!(tree.selected(), None, "asking for a menu chooses nothing");
}

#[test]
fn the_context_menu_key_asks_for_the_cursor_row_s_menu_under_it() {
    let mut tree = Mounted::focused(catalogue());
    tree.key(NamedKey::ArrowDown);
    tree.key(NamedKey::ContextMenu);
    let asked = tree.asked.borrow().clone();
    assert_eq!(asked.len(), 1);
    assert_eq!(&*asked[0].node.id, "layout");
    let around = asked[0].around.expect("opened under the row");
    assert_eq!(around.y, row_height());
    assert_eq!(around.height, row_height());
    assert_eq!(tree.selected(), None);
}

/// A row's wash and corners stand clear of the tree's sides, as a menu's rows do of its panel's.
#[test]
fn the_rows_stand_in_from_the_tree_s_sides() {
    let tree = Mounted::new(catalogue(), 400.0);
    let (view, rows) = laid_rows(&tree);
    assert!(inset() > 0.0);
    for (name, row) in rows {
        assert_eq!(row.x, view.x + inset(), "{name}");
        assert_eq!(row.width, view.width - inset() * 2.0, "{name}");
    }
}

/// Every row stands at the row height of the tree's own size, set in by that size's inset.
#[test]
fn the_rows_stand_at_the_tree_s_size() {
    for size in [ControlSize::Small, ControlSize::Regular, ControlSize::Large] {
        crate::test_support::fresh_layout_runtime();
        let item = tree_view(
            TreeViewProps::props()
                .items(catalogue())
                .label("Components")
                .size(size)
                .build(),
        )
        .unwrap();
        lay_out(item.layout_node(), 240.0, 400.0);
        let tree = ComponentList::new(item);
        let _ = tree.commands();
        let view = crate::test_support::drawn(Role::Tree)[0];
        let rows = crate::test_support::drawn(Role::TreeItem);
        assert!(!rows.is_empty());
        for row in rows {
            assert_eq!(row.height, crate::metrics::row_height(size), "{size:?}");
            assert_eq!(row.x, view.x + crate::metrics::panel_pad(size), "{size:?}");
        }
    }
}

/// The names of the nodes framed in the keyboard's ring.
fn ringed(tree: &Mounted) -> Vec<String> {
    let band = telar::Paint::Solid(telar::focus_ring().band);
    let commands = tree.tree.commands();
    let framed: Vec<Rect> = commands
        .iter()
        .filter_map(|command| match command {
            DrawCommand::Rect { rect, style } => style
                .border
                .is_some_and(|border| border.paint == band)
                .then_some(*rect),
            _ => None,
        })
        .collect();
    telar::inspect::snapshot(&commands)
        .into_iter()
        .filter(|node| node.id.is_some() && framed.contains(&node.rect))
        .map(|node| match node.role {
            Role::Tree => "the tree".to_string(),
            _ => node.name,
        })
        .collect()
}

#[test]
fn an_empty_tree_wears_the_ring_itself_while_the_keyboard_is_on_it() {
    let tree = Mounted::focused(Vec::new());
    assert_eq!(ringed(&tree), ["the tree"]);
}

#[test]
fn a_tree_with_rows_rings_the_cursor_s_row_and_not_itself() {
    let tree = Mounted::focused(catalogue());
    assert_eq!(ringed(&tree), [pointed(&tree).expect("a cursor row")]);
}

#[test]
fn a_branch_with_nothing_under_it_is_still_a_branch() {
    let mut tree = Mounted::focused(vec![
        node("empty", "Empty folder").with_children([]),
        node("note", "Note"),
    ]);
    let empty = |tree: &Mounted| {
        telar::inspect::snapshot(&tree.tree.commands())
            .into_iter()
            .find(|row| row.role == Role::TreeItem && row.name == "Empty folder")
            .expect("the empty branch is shown")
    };
    assert!(empty(&tree).expanded == Some(false), "it says it is shut");
    tree.key(NamedKey::ArrowRight);
    assert_eq!(tree.open(), ["empty"]);
    assert!(empty(&tree).expanded == Some(true), "and opens, showing nothing");
    tree.key(NamedKey::ArrowRight);
    tree.key(NamedKey::Enter);
    assert_eq!(
        tree.selected().as_deref(),
        Some("empty"),
        "right on an open branch with nothing in it stays on it"
    );
}

#[test]
fn what_leading_builds_stands_before_each_row_s_label() {
    let tree = Mounted::shaped(catalogue(), 400.0, |props| {
        props.leading(Rc::new(|node: &Node| {
            let mark = match node.is_branch() {
                true => "▸",
                false => "·",
            };
            Ok(box_item(Text::declaring(
                move || mark.to_string(),
                LayoutStyle::new(),
                |text| text,
            )?))
        }))
    });
    assert_eq!(
        tree.drawn(),
        ["▸", "Inputs", "▸", "Layout", "·", "About"],
        "an icon per row, before its label"
    );
}

type Heard = Rc<RefCell<Vec<(Key, Option<String>)>>>;

/// A focused tree whose app takes F2 and nothing else, recording every key it is asked about with the node the keyboard was on.
fn commanded() -> (Mounted, Heard) {
    let heard = Heard::default();
    let hearing = heard.clone();
    let tree = Mounted::shaped(catalogue(), 400.0, move |props| {
        props.on_key(Rc::new(move |key: &Key, node: Option<&Node>| {
            hearing
                .borrow_mut()
                .push((key.clone(), node.map(|node| node.id.to_string())));
            *key == Key::Named(NamedKey::F2)
        }))
    });
    focus_newest_tree();
    (tree, heard)
}

#[test]
fn the_app_hears_each_key_first_with_the_node_the_keyboard_is_on() {
    let (mut tree, heard) = commanded();
    tree.key(NamedKey::F2);
    tree.key(NamedKey::ArrowDown);
    tree.key(NamedKey::F2);
    assert_eq!(
        *heard.borrow(),
        [
            (Key::Named(NamedKey::F2), Some("inputs".to_string())),
            (Key::Named(NamedKey::ArrowDown), Some("inputs".to_string())),
            (Key::Named(NamedKey::F2), Some("layout".to_string())),
        ],
        "a key the app leaves is the tree's: the arrow moved the cursor"
    );
}

#[test]
fn an_arrow_with_a_modifier_held_is_never_the_tree_s() {
    let (mut tree, heard) = commanded();
    for key in [NamedKey::ArrowDown, NamedKey::ArrowRight, NamedKey::End] {
        tree.send(key_with(Key::Named(key), telar::testing::hold(false, true)));
    }
    telar::testing::hold(false, false);
    tree.key(NamedKey::F2);
    assert_eq!(
        heard.borrow().last(),
        Some(&(Key::Named(NamedKey::F2), Some("inputs".to_string()))),
        "Alt and the arrows moved nothing"
    );
    assert!(tree.open().is_empty(), "and opened nothing");
}

#[derive(Clone, Debug, PartialEq)]
enum Carried {
    Node(Arc<str>),
    Sentence(u32),
}

type Drops = Rc<RefCell<Vec<(Carried, TreeTarget<Arc<str>>)>>>;

/// A tree `height` tall whose rows are carried as [`Carried::Node`], over a 40px box below it that carries `Sentence(7)`, both in one session, with the drops the tree reports.
struct Dragged {
    tree: ComponentList,
    expanded: RwSignal<HashSet<Arc<str>>>,
    drops: Drops,
    session: DragSession<Carried>,
    height: f32,
}

fn dragged(
    items: Vec<Node>,
    height: f32,
    accepts: impl Fn(&Carried, &TreeTarget<Arc<str>>) -> bool + 'static,
) -> Dragged {
    crate::test_support::fresh_layout_runtime();
    telar::testing::reset_keyboard();
    focus::clear();
    let items = signal(items);
    let expanded = signal(HashSet::new());
    let drops = Drops::default();
    let session = DragSession::new();
    let dropping = drops.clone();
    let drag = TreeDrag::new(
        &session,
        |node: &Node| Some(Carried::Node(node.id.clone())),
        accepts,
        move |payload, target| dropping.borrow_mut().push((payload, target)),
    );
    let tree = tree_view(
        TreeViewProps::props()
            .items(Reactive::of(move || items.get()))
            .expanded(expanded)
            .drag(drag)
            .label("Library")
            .build(),
    )
    .unwrap();
    let source = session.item(
        StyledContainer::new(
            LayoutStyle::new().width(240.0).height(40.0),
            |_| RectStyle::default(),
            vec![],
        )
        .unwrap(),
        DragItem::new(|| Some(Carried::Sentence(7))).threshold(0.0),
    );
    let column = Container::new(
        LayoutStyle::column().width(240.0),
        vec![
            box_item(Container::new(LayoutStyle::column().height(height), vec![tree]).unwrap()),
            box_item(source),
        ],
    )
    .unwrap();
    let node = column.layout_node();
    let tree = ComponentList::new(box_item(column));
    lay_out(node, 240.0, height + 40.0);
    let _ = tree.commands();
    Dragged {
        tree,
        expanded,
        drops,
        session,
        height,
    }
}

fn anything(_: &Carried, _: &TreeTarget<Arc<str>>) -> bool {
    true
}

const X: f64 = 120.0;

/// Where on row `index` the pointer is, `along` of the way down it.
fn on_row(index: usize, along: f32) -> f64 {
    ((index as f32 + along) * row_height()) as f64
}

impl Dragged {
    fn send(&mut self, event: Event) {
        route(&mut self.tree, &event);
        self.settle();
    }

    fn settle(&mut self) {
        for _ in 0..4 {
            relayout_if_dirty();
            let _ = self.tree.commands();
        }
    }

    /// Presses at `from`, carries the payload past the drag threshold and on to `to`, and holds it there.
    fn carry(&mut self, from: f64, to: f64) {
        self.send(press(X, from));
        self.send(moved(X, from + 8.0));
        self.send(moved(X, to));
    }

    fn drag(&mut self, from: f64, to: f64) {
        self.carry(from, to);
        self.send(release(X, to));
    }

    fn below(&self) -> f64 {
        (self.height + 20.0) as f64
    }

    fn drops(&self) -> Vec<(Carried, TreeTarget<Arc<str>>)> {
        self.drops.borrow().clone()
    }
}

fn key(id: &str) -> Option<Arc<str>> {
    Some(Arc::from(id))
}

#[test]
fn a_row_dropped_between_two_others_lands_among_their_siblings() {
    let mut tree = dragged(catalogue(), 400.0, anything);
    tree.drag(on_row(2, 0.5), on_row(1, 0.1));
    assert_eq!(
        tree.drops(),
        [(
            Carried::Node(Arc::from("about")),
            TreeTarget::Between {
                parent: None,
                after: key("inputs"),
                before: key("layout"),
            }
        )]
    );
}

/// Rows go into the catalogue's branches and never onto a leaf: what an app whose rows are folders and files says.
fn into_branches(_: &Carried, target: &TreeTarget<Arc<str>>) -> bool {
    match target {
        TreeTarget::Inside(node) => ["inputs", "button", "layout"].contains(&&**node),
        TreeTarget::Between { .. } => true,
    }
}

#[test]
fn a_row_dropped_onto_a_node_the_app_takes_it_into_lands_inside_it() {
    let mut tree = dragged(catalogue(), 400.0, into_branches);
    tree.drag(on_row(2, 0.5), on_row(1, 0.5));
    tree.drag(on_row(1, 0.5), on_row(2, 0.6));
    assert_eq!(
        tree.drops(),
        [
            (
                Carried::Node(Arc::from("about")),
                TreeTarget::Inside(Arc::from("layout"))
            ),
            (
                Carried::Node(Arc::from("layout")),
                TreeTarget::Between {
                    parent: None,
                    after: key("about"),
                    before: None,
                }
            ),
        ],
        "past the middle of a node that takes nothing onto it is the gap after it"
    );
}

#[test]
fn an_empty_branch_takes_a_row_dropped_into_it() {
    let mut tree = dragged(
        vec![node("empty", "Empty").with_children([]), node("note", "Note")],
        400.0,
        anything,
    );
    tree.drag(on_row(1, 0.5), on_row(0, 0.5));
    assert_eq!(
        tree.drops(),
        [(
            Carried::Node(Arc::from("note")),
            TreeTarget::Inside(Arc::from("empty"))
        )]
    );
}

#[test]
fn a_branch_is_never_dropped_into_itself_nor_put_back_where_it_was() {
    let mut tree = dragged(catalogue(), 400.0, anything);
    tree.expanded.set([Arc::from("inputs")].into_iter().collect());
    tree.settle();
    tree.drag(on_row(0, 0.5), on_row(1, 0.5));
    tree.drag(on_row(0, 0.5), on_row(2, 0.1));
    tree.drag(on_row(4, 0.5), on_row(4, 0.9));
    assert!(
        tree.drops().is_empty(),
        "into a branch under it, among what it holds, or beside itself: {:?}",
        tree.drops()
    );
}

#[test]
fn held_over_a_shut_branch_the_pointer_opens_it() {
    let mut tree = dragged(catalogue(), 400.0, anything);
    tree.carry(on_row(2, 0.5), on_row(1, 0.5));
    advance_time(Duration::from_millis(300));
    assert!(tree.expanded.get().is_empty(), "not before the hold delay");
    advance_time(Duration::from_millis(600));
    tree.settle();
    assert_eq!(
        tree.expanded.get(),
        [Arc::from("layout")].into_iter().collect()
    );
    tree.send(release(X, on_row(1, 0.5)));
    assert_eq!(
        tree.drops(),
        [(
            Carried::Node(Arc::from("about")),
            TreeTarget::Inside(Arc::from("layout"))
        )]
    );
}

/// Before `inputs` opens, the lower half of its row is the gap before `layout`, at the top level; once it opens under a pointer that does not move, the same gap is the first among what `inputs` holds, which this app refuses.
#[test]
fn a_branch_opened_under_a_still_pointer_is_measured_again() {
    let mut tree = dragged(catalogue(), 400.0, |_, target| match target {
        TreeTarget::Inside(_) => false,
        TreeTarget::Between { parent, .. } => parent.as_deref() != Some("inputs"),
    });
    tree.carry(on_row(2, 0.5), on_row(0, 0.85));
    assert!(tree.session.over().is_some(), "the gap before `layout`");
    advance_time(Duration::from_millis(800));
    tree.settle();
    assert!(tree.expanded.get().contains("inputs"));
    assert_eq!(
        tree.session.over(),
        None,
        "the gap now lands among what `inputs` holds"
    );
    tree.send(release(X, on_row(0, 0.85)));
    assert!(tree.drops().is_empty());
}

/// A sentence from a list beside the tree lands on `about`, a leaf, as a sentence lands on a section of a text, and nowhere else.
#[test]
fn a_payload_from_another_widget_lands_on_the_node_the_app_accepts_it_on() {
    let mut tree = dragged(catalogue(), 400.0, |payload, target| {
        matches!(
            (payload, target),
            (Carried::Sentence(_), TreeTarget::Inside(node)) if &**node == "about"
        )
    });
    let below = tree.below();
    tree.carry(below, on_row(1, 0.5));
    assert_eq!(tree.session.over(), None, "`layout` takes no sentence");
    tree.send(moved(X, on_row(2, 0.5)));
    tree.send(release(X, on_row(2, 0.5)));
    assert_eq!(
        tree.drops(),
        [(Carried::Sentence(7), TreeTarget::Inside(Arc::from("about")))]
    );
}

#[test]
fn escape_mid_drag_drops_nothing() {
    let mut tree = dragged(catalogue(), 400.0, anything);
    tree.carry(on_row(2, 0.5), on_row(1, 0.5));
    tree.send(named(NamedKey::Escape));
    assert_eq!(tree.session.carried(), None);
    tree.send(moved(X, on_row(0, 0.5)));
    tree.send(release(X, on_row(0, 0.5)));
    assert!(tree.drops().is_empty());
}

fn names_built(tree: &ComponentList) -> Vec<String> {
    telar::inspect::snapshot(&tree.commands())
        .into_iter()
        .filter(|node| node.role == Role::TreeItem)
        .map(|node| node.name)
        .collect()
}

/// The row picked up scrolls out of the rows built while the list scrolls under the pointer, and the stroke goes on without it.
#[test]
fn a_row_carried_past_the_rows_built_is_still_dropped() {
    let rows: Vec<Node> = (0..200)
        .map(|i| node(format!("row {i}"), format!("Row {i}")))
        .collect();
    let height = row_height() * 10.0;
    let mut tree = dragged(rows, height, |_, target| {
        matches!(target, TreeTarget::Between { .. })
    });
    tree.carry(on_row(0, 0.5), (height - 2.0) as f64);
    for _ in 0..40 {
        advance_time(Duration::from_millis(50));
        tree.settle();
    }
    assert!(
        !names_built(&tree.tree).contains(&"Row 0".to_string()),
        "the row picked up is no longer built"
    );
    assert_eq!(tree.session.carried(), Some(Carried::Node(Arc::from("row 0"))));
    tree.send(release(X, (height - 2.0) as f64));
    let drops = tree.drops();
    assert_eq!(drops.len(), 1, "{drops:?}");
    let (payload, TreeTarget::Between { after, .. }) = &drops[0] else {
        panic!("dropped between two rows: {drops:?}");
    };
    assert_eq!(*payload, Carried::Node(Arc::from("row 0")));
    let after: usize = after
        .as_deref()
        .and_then(|after| after.strip_prefix("row "))
        .and_then(|n| n.parse().ok())
        .expect("after a row");
    assert!(after > 20, "landed past what was in view at the press: row {after}");
}
