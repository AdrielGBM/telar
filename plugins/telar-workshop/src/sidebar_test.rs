use telar::preview::PreviewCtx;
use telar::testing::{
    Scoped, centre, find_text, key_with, mount, named, press, release, route, texts,
};
use telar::{
    AccessNode, App, AppRuntime, Component, ComponentList, Container, Event, LayoutStyle, LocalApp,
    PointerSource, Rect, WindowRoot, reset_layout_runtime,
};

use super::*;
use crate::state::ViewMode;
use crate::test_support::settle;

fn blank(_: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    Ok(box_item(Container::new(LayoutStyle::new(), Vec::new())?))
}

fn entries() -> Vec<PreviewEntry> {
    vec![
        PreviewEntry::new("button--primary", "button", "Primary", blank)
            .title("Inputs/Button")
            .tags(&["form"]),
        PreviewEntry::new("button--secondary", "button", "Secondary", blank).title("Inputs/Button"),
        PreviewEntry::new("checkbox--checked", "checkbox", "Checked", blank)
            .title("Inputs/Checkbox"),
        PreviewEntry::new("modal--open", "modal", "Open", blank).title("Overlays/Modal"),
    ]
}

/// The sidebar alone, as an application whose access tree a test can read.
struct Workshop {
    state: WorkshopState,
}

impl App for Workshop {
    fn root(&self) -> Box<dyn Component> {
        Box::new(WindowRoot::wrapping(sidebar(&self.state).unwrap()).unwrap())
    }
}

struct Mounted {
    runtime: LocalApp<Workshop>,
    tree: Scoped<ComponentList>,
}

impl Mounted {
    /// The sidebar with every group open, so each row is on screen.
    fn new(entries: Vec<PreviewEntry>) -> Self {
        Self::mount(entries, true)
    }

    fn first_run(entries: Vec<PreviewEntry>) -> Self {
        Self::mount(entries, false)
    }

    fn mount(entries: Vec<PreviewEntry>, every_group_open: bool) -> Self {
        telar::testing::install_test_face();
        reset_layout_runtime();
        focus::clear();
        let state = WorkshopState::new(entries.into(), &Default::default());
        if every_group_open {
            state
                .expanded()
                .set(group_ids(state.entries()).into_iter().collect());
        }
        let runtime = LocalApp(Workshop { state });
        let tree = mount(|| runtime.0.root(), 240, 600);
        let mounted = Self { runtime, tree };
        settle(&mounted.tree);
        mounted
    }

    fn state(&self) -> &WorkshopState {
        &self.runtime.0.state
    }

    fn send(&mut self, event: Event) {
        route(&mut self.tree, &event);
        settle(&self.tree);
    }

    fn key(&mut self, key: NamedKey) {
        self.send(named(key));
    }

    fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            self.send(key_with(Key::Char(c), Default::default()));
        }
    }

    fn tab(&mut self, times: usize) {
        for _ in 0..times {
            self.key(NamedKey::Tab);
        }
    }

    fn focus_tree(&mut self) {
        self.tab(2);
    }

    fn drawn(&self, text: &str) -> bool {
        find_text(&self.tree, text)
    }

    fn selected(&self) -> Option<String> {
        self.state().selection().get().as_deref().map(str::to_owned)
    }

    fn search(&self, query: &str) {
        self.state().search().set(query.to_owned());
        settle(&self.tree);
    }

    fn row(&self, name: &str) -> AccessNode {
        let nodes = self.runtime.access_snapshot(&self.tree.commands());
        nodes
            .iter()
            .find(|node| node.role == Role::TreeItem && names(node, name))
            .cloned()
            .unwrap_or_else(|| {
                let rows: Vec<_> = nodes
                    .iter()
                    .filter(|node| node.role == Role::TreeItem)
                    .map(|node| node.name.as_str())
                    .collect();
                panic!("no row named {name:?} among {rows:?}")
            })
    }

    fn click(&mut self, rect: Rect) {
        let (x, y) = centre(rect);
        self.send(press(x, y));
        self.send(release(x, y));
    }
}

#[test]
fn previews_are_grouped_by_their_title_path() {
    let sidebar = Mounted::new(entries());
    let shown = texts(&sidebar.tree);
    let at = |text: &str| {
        shown
            .iter()
            .position(|shown| shown == text)
            .unwrap_or_else(|| panic!("{text} is not drawn in {shown:?}"))
    };
    assert!(at("Inputs") < at("Button"));
    assert!(at("Button") < at("Primary"));
    assert!(at("Primary") < at("Secondary"));
    assert!(at("Secondary") < at("Checkbox"));
    assert!(at("Checkbox") < at("Overlays"));
}

#[test]
fn a_group_counts_its_previews() {
    let sidebar = Mounted::new(entries());
    let shown = texts(&sidebar.tree);
    let after = |label: &str| {
        let at = shown.iter().position(|shown| shown == label).unwrap();
        shown[at + 1].clone()
    };
    assert_eq!(after("Inputs"), "3");
    assert_eq!(after("Button"), "2");
    assert_eq!(after("Overlays"), "1");
}

#[test]
fn a_first_run_opens_only_the_groups_holding_the_selection() {
    let sidebar = Mounted::first_run(entries());
    assert!(sidebar.drawn("Primary"));
    assert!(sidebar.drawn("Checkbox"));
    assert!(!sidebar.drawn("Checked"));
    assert!(sidebar.drawn("Overlays"));
    assert!(!sidebar.drawn("Modal"));
}

#[test]
fn shutting_a_group_shuts_the_groups_inside_it() {
    let mut sidebar = Mounted::new(entries());
    sidebar.focus_tree();
    sidebar.key(NamedKey::Home);
    sidebar.key(NamedKey::ArrowLeft);
    sidebar.key(NamedKey::ArrowRight);
    assert!(sidebar.drawn("Button"));
    assert!(!sidebar.drawn("Primary"));
    assert!(!sidebar.drawn("Checked"));
}

#[test]
fn a_group_collapses_and_opens_again() {
    let mut sidebar = Mounted::new(entries());
    sidebar.focus_tree();
    sidebar.key(NamedKey::End);
    sidebar.key(NamedKey::ArrowUp);
    sidebar.key(NamedKey::ArrowUp);
    sidebar.key(NamedKey::Enter);
    assert!(!sidebar.drawn("Open"));
    assert!(!sidebar.state().expanded().get().contains("/Overlays"));
    sidebar.key(NamedKey::Enter);
    assert!(sidebar.drawn("Modal"));
    assert!(!sidebar.drawn("Open"), "the group inside it opens shut");
    assert_eq!(sidebar.selected().as_deref(), Some("button--primary"));
}

#[test]
fn a_search_narrows_by_title_name_and_tags() {
    let sidebar = Mounted::new(entries());
    sidebar.search("modal");
    assert!(sidebar.drawn("Open"));
    assert!(!sidebar.drawn("Primary"));
    sidebar.search("prm");
    assert!(sidebar.drawn("Primary"));
    assert!(!sidebar.drawn("Secondary"));
    sidebar.search("form");
    assert!(sidebar.drawn("Primary"));
    assert!(!sidebar.drawn("Checked"));
    sidebar.search("inputs sec");
    assert!(sidebar.drawn("Secondary"));
    assert!(!sidebar.drawn("Primary"));
    sidebar.search("dom");
    assert!(!sidebar.drawn("Open"));
}

#[test]
fn a_search_opens_the_groups_a_collapse_shut() {
    let mut sidebar = Mounted::new(entries());
    sidebar.focus_tree();
    sidebar.key(NamedKey::End);
    sidebar.key(NamedKey::ArrowUp);
    sidebar.key(NamedKey::ArrowUp);
    sidebar.key(NamedKey::Enter);
    assert!(!sidebar.drawn("Open"));
    sidebar.search("modal");
    assert!(sidebar.drawn("Open"));
}

#[test]
fn no_matches_offers_to_clear_the_search() {
    let mut sidebar = Mounted::new(entries());
    sidebar.search("zzz");
    assert!(sidebar.drawn("No matches"));
    assert!(!sidebar.drawn("Primary"));
    sidebar.tab(2);
    sidebar.key(NamedKey::Enter);
    assert_eq!(sidebar.state().search().get(), "");
    assert!(sidebar.drawn("Primary"));
    assert!(!sidebar.drawn("No matches"));
}

#[test]
fn the_selection_follows_enter_on_a_row() {
    let mut sidebar = Mounted::new(entries());
    assert_eq!(sidebar.selected().as_deref(), Some("button--primary"));
    sidebar.focus_tree();
    sidebar.key(NamedKey::ArrowDown);
    sidebar.key(NamedKey::Enter);
    assert_eq!(sidebar.selected().as_deref(), Some("button--secondary"));
}

#[test]
fn the_arrows_walk_the_rows_and_enter_selects() {
    let mut sidebar = Mounted::new(entries());
    sidebar.focus_tree();
    sidebar.key(NamedKey::ArrowDown);
    assert_eq!(sidebar.selected().as_deref(), Some("button--primary"));
    sidebar.key(NamedKey::Enter);
    assert_eq!(sidebar.selected().as_deref(), Some("button--secondary"));
    sidebar.key(NamedKey::End);
    sidebar.key(NamedKey::Enter);
    assert_eq!(sidebar.selected().as_deref(), Some("modal--open"));
    sidebar.key(NamedKey::ArrowUp);
    sidebar.key(NamedKey::ArrowUp);
    sidebar.key(NamedKey::Enter);
    assert!(!sidebar.drawn("Open"));
    sidebar.key(NamedKey::Home);
    sidebar.key(NamedKey::ArrowLeft);
    assert!(!sidebar.drawn("Checked"));
    sidebar.key(NamedKey::ArrowRight);
    assert!(sidebar.drawn("Checkbox"));
    assert!(!sidebar.drawn("Checked"));
}

#[test]
fn clicking_a_preview_selects_it_and_clicking_a_group_shuts_it() {
    let mut sidebar = Mounted::new(entries());
    let secondary = sidebar.row("Secondary").rect;
    sidebar.click(secondary);
    assert_eq!(sidebar.selected().as_deref(), Some("button--secondary"));

    let overlays = sidebar.row("Overlays").rect;
    sidebar.click(overlays);
    assert!(!sidebar.drawn("Open"));
    assert_eq!(sidebar.selected().as_deref(), Some("button--secondary"));
}

#[test]
fn enter_in_the_search_selects_the_first_match() {
    let mut sidebar = Mounted::new(entries());
    sidebar.tab(1);
    sidebar.type_text("modal");
    sidebar.key(NamedKey::Enter);
    assert_eq!(sidebar.selected().as_deref(), Some("modal--open"));
}

#[test]
fn an_empty_workshop_lists_nothing() {
    let sidebar = Mounted::new(Vec::new());
    assert!(sidebar.drawn("No previews"));
    assert!(!sidebar.drawn("Clear search"));
}

/// Whether `node` is the row labelled `label`: a row is announced by its label, then its badge.
fn names(node: &AccessNode, label: &str) -> bool {
    node.name
        .strip_prefix(label)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(' '))
}

#[test]
fn the_search_field_is_a_full_size_target() {
    let mounted = Mounted::new(entries());
    let nodes = mounted.runtime.access_snapshot(&mounted.tree.commands());
    let field = nodes
        .iter()
        .find(|node| node.role == Role::TextInput)
        .expect("the sidebar has a search field");
    assert!(
        field.rect.height >= telar::inspect::a11y::MIN_TARGET,
        "the search field is {:?}",
        field.rect
    );
}

#[test]
fn escape_clears_the_search_and_a_second_escape_leaves_for_the_tree() {
    let mut sidebar = Mounted::new(entries());
    sidebar.tab(1);
    sidebar.type_text("modal");
    sidebar.key(NamedKey::Escape);
    assert_eq!(sidebar.state().search().get(), "");
    assert!(sidebar.drawn("Primary"));
    assert!(focus::text_entry_focused());
    sidebar.key(NamedKey::Escape);
    assert!(!focus::text_entry_focused());
}

#[test]
fn arrow_down_in_the_search_hands_the_keyboard_to_the_tree() {
    let mut sidebar = Mounted::new(entries());
    sidebar.tab(1);
    assert!(focus::text_entry_focused());
    sidebar.key(NamedKey::ArrowDown);
    assert!(!focus::text_entry_focused());
}

#[test]
fn the_clear_button_empties_the_search() {
    let mut sidebar = Mounted::new(entries());
    sidebar.search("modal");
    let nodes = sidebar.runtime.access_snapshot(&sidebar.tree.commands());
    let clear = nodes
        .iter()
        .find(|node| node.role == Role::Button && node.name == "Clear search")
        .expect("a clear button shows while there is a query")
        .rect;
    sidebar.click(clear);
    assert_eq!(sidebar.state().search().get(), "");
}

#[test]
fn focus_search_takes_the_keyboard_to_the_field() {
    let sidebar = Mounted::new(entries());
    sidebar.state().focus_search();
    settle(&sidebar.tree);
    assert!(focus::text_entry_focused());
}

fn packaged_entries() -> Vec<PreviewEntry> {
    vec![
        PreviewEntry::new("app--button--primary", "button", "Primary", blank)
            .package("app")
            .title("Inputs/Button"),
        PreviewEntry::new("kit--button--ghost", "button", "Ghost", blank)
            .package("kit")
            .title("Inputs/Button"),
        PreviewEntry::new("kit--modal--open", "modal", "Open", blank)
            .package("kit")
            .title("Overlays/Modal"),
    ]
}

#[test]
fn previews_of_more_than_one_package_are_listed_under_their_package_in_the_order_they_come() {
    let sidebar = Mounted::new(packaged_entries());
    let shown = texts(&sidebar.tree);
    let at = |text: &str| {
        shown
            .iter()
            .position(|shown| shown == text)
            .unwrap_or_else(|| panic!("{text} is not drawn in {shown:?}"))
    };
    assert!(at("app") < at("Primary"));
    assert!(at("Primary") < at("kit"));
    assert!(at("kit") < at("Ghost"));
    assert!(at("Ghost") < at("Overlays"));
    assert_eq!(shown[at("kit") + 1], "2");
    let expanded = sidebar.state().expanded().get();
    assert!(expanded.contains("/kit/Inputs/Button"));
    assert!(expanded.contains("/app/Inputs/Button"));
}

#[test]
fn previews_of_one_package_are_listed_without_it() {
    let one_package = packaged_entries()
        .into_iter()
        .filter(|entry| entry.package == "kit")
        .collect();
    let sidebar = Mounted::new(one_package);
    assert!(!sidebar.drawn("kit"));
    assert!(sidebar.drawn("Inputs"));
}

#[test]
fn a_search_finds_previews_by_their_package_and_keeps_the_package_level() {
    let sidebar = Mounted::new(packaged_entries());
    sidebar.search("app");
    assert!(sidebar.drawn("app"));
    assert!(sidebar.drawn("Primary"));
    assert!(!sidebar.drawn("Ghost"));
}

impl Mounted {
    fn secondary_click(&mut self, rect: Rect) {
        let (x, y) = centre(rect);
        let button = PointerButton::Secondary;
        self.send(Event::PointerPressed {
            x,
            y,
            button,
            source: PointerSource::Mouse,
        });
        self.send(Event::PointerReleased {
            x,
            y,
            button,
            source: PointerSource::Mouse,
        });
    }

    fn menu(&self) -> Vec<String> {
        self.runtime
            .access_snapshot(&self.tree.commands())
            .into_iter()
            .filter(|node| node.role == Role::MenuItem)
            .map(|node| node.name)
            .collect()
    }

    fn pick(&mut self, name: &str) {
        let row = self
            .runtime
            .access_snapshot(&self.tree.commands())
            .into_iter()
            .find(|node| node.role == Role::MenuItem && node.name == name)
            .unwrap_or_else(|| panic!("no menu row named {name:?} among {:?}", self.menu()));
        self.click(row.rect);
    }
}

#[test]
fn a_group_s_menu_opens_or_shuts_it_and_everything_else() {
    let mut sidebar = Mounted::new(entries());
    let inputs = sidebar.row("Inputs").rect;
    sidebar.secondary_click(inputs);
    assert_eq!(
        sidebar.menu(),
        [
            "Expand the group",
            "Collapse the group",
            "Expand all",
            "Collapse all"
        ]
    );
    sidebar.pick("Collapse the group");
    let open = sidebar.state().expanded().get();
    assert!(!open.contains("/Inputs") && !open.contains("/Inputs/Button"));
    assert!(open.contains("/Overlays"), "the other groups stay open");
    assert!(sidebar.menu().is_empty(), "picking a row closes the menu");

    sidebar.secondary_click(sidebar.row("Overlays").rect);
    sidebar.pick("Collapse all");
    assert!(sidebar.state().expanded().get().is_empty());
    assert!(!sidebar.drawn("Primary"));

    sidebar.secondary_click(sidebar.row("Inputs").rect);
    sidebar.pick("Expand all");
    assert_eq!(
        sidebar.state().expanded().get(),
        crate::sidebar::group_ids(sidebar.state().entries())
            .into_iter()
            .collect()
    );
    assert!(sidebar.drawn("Primary"));
}

#[test]
fn a_preview_s_menu_shows_it_in_a_view_and_selects_it() {
    let mut sidebar = Mounted::new(entries());
    sidebar.secondary_click(sidebar.row("Open").rect);
    assert_eq!(
        sidebar.menu(),
        [
            "Show the canvas",
            "Show the matrix",
            "Show the docs",
            "Copy link",
            "Expand all",
            "Collapse all"
        ]
    );
    assert_eq!(
        sidebar.selected().as_deref(),
        Some("button--primary"),
        "asking for the menu chooses nothing"
    );
    sidebar.pick("Show the docs");
    assert_eq!(sidebar.selected().as_deref(), Some("modal--open"));
    assert_eq!(sidebar.state().view().get(), ViewMode::Docs);
}

#[test]
fn the_space_under_the_rows_offers_what_concerns_every_group() {
    let mut sidebar = Mounted::new(entries());
    sidebar.secondary_click(Rect::new(20.0, 560.0, 10.0, 10.0));
    assert_eq!(sidebar.menu(), ["Expand all", "Collapse all"]);
}

#[test]
fn the_context_menu_key_opens_the_menu_of_the_row_the_keyboard_is_on() {
    let mut sidebar = Mounted::new(entries());
    sidebar.focus_tree();
    sidebar.key(NamedKey::ContextMenu);
    assert_eq!(
        sidebar.menu(),
        [
            "Show the canvas",
            "Show the matrix",
            "Show the docs",
            "Copy link",
            "Expand all",
            "Collapse all"
        ],
        "the keyboard arrives on the selected preview"
    );
    sidebar.key(NamedKey::Escape);
    assert!(sidebar.menu().is_empty());
}

#[test]
fn a_preview_written_in_a_known_file_offers_to_open_it_in_the_editor() {
    let mut sidebar = Mounted::new(vec![
        PreviewEntry::new("card--plain", "card", "Plain", blank)
            .title("Display/Card")
            .location("/src/card.rsx", 12),
    ]);
    sidebar.secondary_click(sidebar.row("Plain").rect);
    assert!(sidebar.menu().contains(&"Open in editor".to_string()));
}
