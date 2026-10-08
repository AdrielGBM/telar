use telar::preview::PreviewCtx;
use telar::testing::{find_text, mount, texts};
use telar::{
    AccessNode, App, AppRuntime, Component, ComponentList, Container, Event, LayoutStyle, LocalApp,
    PointerButton, PointerSource, Rect, WindowRoot, relayout_if_dirty, reset_layout_runtime,
};
use telar_devtools::workbench_scope;

use super::*;

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

/// The sidebar alone, or with the shell around it, as an application whose access tree a test can read.
struct Workshop {
    state: WorkshopState,
    shell: bool,
}

impl App for Workshop {
    fn root(&self) -> Box<dyn Component> {
        let built = if self.shell {
            workbench_scope(|| crate::shell::shell(&self.state, None)).map(box_item)
        } else {
            sidebar(&self.state)
        };
        Box::new(WindowRoot::wrapping(built.unwrap()).unwrap())
    }
}

struct Mounted {
    runtime: LocalApp<Workshop>,
    tree: ComponentList,
}

impl Mounted {
    fn new(entries: Vec<PreviewEntry>) -> Self {
        Self::mount(entries, false, 240, 600)
    }

    fn in_shell(entries: Vec<PreviewEntry>) -> Self {
        Self::mount(entries, true, 1000, 700)
    }

    fn mount(entries: Vec<PreviewEntry>, shell: bool, width: u32, height: u32) -> Self {
        reset_layout_runtime();
        focus::clear();
        let state = WorkshopState::new(entries.into(), &Default::default());
        let runtime = LocalApp(Workshop { state, shell });
        let tree = mount(runtime.0.root(), width, height);
        let mounted = Self { runtime, tree };
        mounted.settle();
        mounted
    }

    fn state(&self) -> &WorkshopState {
        &self.runtime.0.state
    }

    fn settle(&self) {
        relayout_if_dirty();
        let _ = self.tree.commands();
    }

    fn send(&mut self, event: Event) {
        if !telar::dispatch_overlays(&event) {
            self.tree.on_event(&event);
        }
        self.settle();
    }

    fn key(&mut self, key: NamedKey) {
        self.send(Event::KeyPressed {
            key: Key::Named(key),
            modifiers: Default::default(),
        });
    }

    fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            self.send(Event::KeyPressed {
                key: Key::Char(c),
                modifiers: Default::default(),
            });
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
        self.settle();
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

    fn focused(&self) -> Option<Role> {
        self.runtime
            .access_snapshot(&self.tree.commands())
            .into_iter()
            .find(|node| node.focused)
            .map(|node| node.role)
    }

    fn click(&mut self, rect: Rect) {
        let (x, y) = (
            f64::from(rect.x + rect.width / 2.0),
            f64::from(rect.y + rect.height / 2.0),
        );
        for event in [
            Event::PointerPressed {
                x,
                y,
                button: PointerButton::Primary,
                source: PointerSource::Mouse,
            },
            Event::PointerReleased {
                x,
                y,
                button: PointerButton::Primary,
                source: PointerSource::Mouse,
            },
        ] {
            self.send(event);
        }
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
    assert!(sidebar.drawn("Open"));
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
    assert!(sidebar.drawn("Checked"));
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
fn slash_takes_the_keyboard_to_the_search_without_typing_itself() {
    let mut workshop = Mounted::in_shell(entries());
    while workshop.focused() != Some(Role::Tree) {
        workshop.tab(1);
    }
    workshop.type_text("/");
    assert_eq!(workshop.state().search().get(), "");
    workshop.type_text("mo/");
    assert_eq!(workshop.state().search().get(), "mo/");
    assert_eq!(workshop.selected().as_deref(), Some("button--primary"));
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
