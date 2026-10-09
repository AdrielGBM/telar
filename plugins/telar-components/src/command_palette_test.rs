use std::cell::RefCell;

use telar::testing::{centre, key_with, named, press, release, route, texts};
use telar::{
    AvailableSpace, ComponentList, Event, compute_layout, focus, new_container, relayout_if_dirty,
};

use super::*;

fn catalogue() -> Vec<Command> {
    vec![
        Command::new("open", "Open file")
            .in_group("File")
            .with_shortcut("Mod+O"),
        Command::new("save", "Save")
            .in_group("File")
            .with_shortcut("Mod+S"),
        Command::new("theme", "Toggle dark mode")
            .in_group("View")
            .with_keywords(["theme", "appearance"]),
        Command::new("zoom", "Zoom in")
            .in_group("View")
            .disabled(true),
        Command::new("about", "About"),
    ]
}

fn labels(entries: &[Entry]) -> Vec<String> {
    entries
        .iter()
        .map(|entry| match entry {
            Entry::Heading(group) => format!("# {group}"),
            Entry::Item { command, .. } => command.label.to_string(),
        })
        .collect()
}

#[test]
fn with_nothing_typed_every_command_shows_in_its_group_in_order() {
    assert_eq!(
        labels(&rank(&catalogue(), "")),
        [
            "# File",
            "Open file",
            "Save",
            "# View",
            "Toggle dark mode",
            "Zoom in",
            "About"
        ]
    );
}

#[test]
fn typing_keeps_the_matches_best_first_and_leads_with_the_best_group() {
    assert_eq!(
        labels(&rank(&catalogue(), "save")),
        ["# File", "Save"],
        "only what matches is listed"
    );
    assert_eq!(
        labels(&rank(&catalogue(), "o")),
        [
            "# File",
            "Open file",
            "# View",
            "Toggle dark mode",
            "Zoom in",
            "About"
        ],
        "a group is led by its best match, and the groups by theirs"
    );
    assert_eq!(labels(&rank(&catalogue(), "zin"))[1], "Zoom in");
}

#[test]
fn a_keyword_or_the_group_finds_a_command_its_name_does_not_spell() {
    assert_eq!(
        labels(&rank(&catalogue(), "appearance")),
        ["# View", "Toggle dark mode"]
    );
    assert_eq!(labels(&rank(&catalogue(), "file save")), ["# File", "Save"]);
}

#[test]
fn the_matched_characters_of_the_name_are_marked() {
    let ranked = rank(&catalogue(), "opf");
    let Entry::Item { command, marks } = &ranked[1] else {
        panic!("a command follows its heading");
    };
    assert_eq!(&*command.id, "open");
    assert_eq!(marks, &[0..2, 5..6]);
}

struct Mounted {
    tree: ComponentList,
    open: RwSignal<bool>,
    query: RwSignal<String>,
    ran: Rc<RefCell<Vec<String>>>,
}

impl Mounted {
    fn new() -> Self {
        Self::with(catalogue())
    }

    fn with(commands: Vec<Command>) -> Self {
        crate::test_support::fresh_layout_runtime();
        telar::set_locale("en");
        ui_core::reset_keyboard();
        focus::clear();
        let open = signal(false);
        let query = signal(String::new());
        let ran = Rc::new(RefCell::new(Vec::new()));
        let sink = ran.clone();
        let palette = command_palette(
            CommandPaletteProps::props()
                .open(open)
                .query(query)
                .commands(commands)
                .on_run(Rc::new(move |command: &Command| {
                    sink.borrow_mut().push(command.id.to_string())
                }))
                .build(),
            Children::default(),
        )
        .unwrap();
        let root = new_container(
            LayoutStyle::new().flex_column().width(800.0).height(600.0),
            &[palette.layout_node()],
        )
        .unwrap();
        compute_layout(
            root,
            AvailableSpace::Definite(800.0),
            AvailableSpace::Definite(600.0),
        )
        .unwrap();
        let tree = ComponentList::new(palette);
        let mut mounted = Self {
            tree,
            open,
            query,
            ran,
        };
        mounted.settle();
        mounted
    }

    fn opened() -> Self {
        let mut mounted = Self::new();
        mounted.open.set(true);
        mounted.settle();
        mounted
    }

    fn settle(&mut self) {
        relayout_if_dirty();
        let _ = self.tree.commands();
        relayout_if_dirty();
    }

    fn send(&mut self, event: Event) {
        route(&mut self.tree, &event);
        self.settle();
    }

    fn key(&mut self, key: NamedKey) {
        self.send(named(key));
    }

    fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            self.send(key_with(Key::Char(c), Default::default()));
        }
    }

    fn drawn(&self) -> Vec<String> {
        texts(&self.tree)
    }

    /// The name of the row the focused field points a reader at.
    fn pointed(&self) -> Option<String> {
        let nodes = ui_core::accessibility::snapshot(&self.tree.commands());
        let field = nodes
            .iter()
            .find(|node| node.role == Role::TextInput && node.focused)?;
        let row = field.active_descendant?;
        nodes
            .iter()
            .find(|node| node.id == Some(row))
            .map(|node| node.name.clone())
    }
}

#[test]
fn a_closed_palette_draws_nothing() {
    let mounted = Mounted::new();
    assert!(!mounted.drawn().iter().any(|text| text == "Open file"));
}

#[test]
fn opening_it_lists_the_commands_with_their_groups_and_shortcuts() {
    let mounted = Mounted::opened();
    let drawn = mounted.drawn();
    for expected in [
        "File",
        "Open file",
        "Save",
        "View",
        "Toggle dark mode",
        "About",
    ] {
        assert!(
            drawn.iter().any(|text| text == expected),
            "{expected}: {drawn:?}"
        );
    }
    assert!(
        drawn.iter().any(|text| text == "O"),
        "the shortcut's caps: {drawn:?}"
    );
}

#[test]
fn the_keyboard_lands_in_the_field_and_typing_filters_the_list() {
    let mut mounted = Mounted::opened();
    mounted.type_text("tog");
    assert_eq!(mounted.query.get(), "tog");
    let drawn = mounted.drawn();
    assert!(drawn.iter().any(|text| text == "Toggle dark mode"));
    assert!(!drawn.iter().any(|text| text == "Save"), "{drawn:?}");
}

#[test]
fn the_arrows_walk_the_list_past_what_cannot_run_and_enter_runs_it() {
    let mut mounted = Mounted::opened();
    mounted.key(NamedKey::ArrowDown);
    mounted.key(NamedKey::ArrowDown);
    mounted.key(NamedKey::ArrowDown);
    mounted.key(NamedKey::Enter);
    assert_eq!(
        *mounted.ran.borrow(),
        ["about"],
        "the disabled row was passed over"
    );
    assert!(!mounted.open.get(), "running a command closes the palette");
}

#[test]
fn up_from_the_first_row_wraps_to_the_last() {
    let mut mounted = Mounted::opened();
    mounted.key(NamedKey::ArrowUp);
    mounted.key(NamedKey::Enter);
    assert_eq!(*mounted.ran.borrow(), ["about"]);
}

#[test]
fn typing_puts_the_cursor_back_on_the_best_match() {
    let mut mounted = Mounted::opened();
    mounted.key(NamedKey::ArrowDown);
    mounted.key(NamedKey::ArrowDown);
    mounted.type_text("o");
    mounted.key(NamedKey::Enter);
    assert_eq!(*mounted.ran.borrow(), ["open"]);
}

#[test]
fn a_reader_is_pointed_at_the_row_the_cursor_is_on() {
    let mut mounted = Mounted::opened();
    let first = mounted.pointed().expect("the field points at a row");
    assert!(first.contains("Open file"), "{first}");
    mounted.key(NamedKey::ArrowDown);
    let second = mounted.pointed().expect("and follows the cursor");
    assert!(second.contains("Save"), "{second}");
}

#[test]
fn escape_closes_it_and_the_query_is_cleared_for_next_time() {
    let mut mounted = Mounted::opened();
    mounted.type_text("zz");
    mounted.key(NamedKey::Escape);
    assert!(!mounted.open.get());
    assert_eq!(mounted.query.get(), "");
    assert!(mounted.ran.borrow().is_empty());
}

#[test]
fn nothing_matching_says_so() {
    let mut mounted = Mounted::opened();
    mounted.type_text("qqq");
    assert!(
        mounted.drawn().iter().any(|text| text == "No results"),
        "{:?}",
        mounted.drawn()
    );
    mounted.key(NamedKey::Enter);
    assert!(mounted.ran.borrow().is_empty());
    assert!(mounted.open.get(), "Enter on nothing leaves it open");
}

#[test]
fn clicking_a_row_runs_its_command_and_clicking_the_scrim_runs_nothing() {
    let mut mounted = Mounted::opened();
    let save = ui_core::accessibility::snapshot(&mounted.tree.commands())
        .into_iter()
        .find(|node| node.role == Role::MenuItem && node.name.starts_with("Save"))
        .expect("the row is on screen")
        .rect;
    let (x, y) = centre(save);
    mounted.send(press(x, y));
    mounted.send(release(x, y));
    assert_eq!(*mounted.ran.borrow(), ["save"]);
    assert!(!mounted.open.get());

    mounted.open.set(true);
    mounted.settle();
    mounted.send(press(4.0, 590.0));
    mounted.send(release(4.0, 590.0));
    assert!(!mounted.open.get(), "the scrim dismisses");
    assert_eq!(*mounted.ran.borrow(), ["save"]);
}

#[test]
fn a_reader_hears_the_dialog_s_name_and_each_row_with_its_shortcut() {
    let mounted = Mounted::opened();
    let nodes = ui_core::accessibility::snapshot(&mounted.tree.commands());
    assert!(
        nodes
            .iter()
            .any(|node| node.role == Role::Dialog && node.name == "Command palette"),
        "the dialog is named"
    );
    let command = if cfg!(target_os = "macos") {
        "⌘"
    } else {
        "Ctrl"
    };
    let rows: Vec<&str> = nodes
        .iter()
        .filter(|node| node.role == Role::MenuItem)
        .map(|node| node.name.as_str())
        .collect();
    assert_eq!(
        rows,
        [
            format!("Open file, {command}+O").as_str(),
            &format!("Save, {command}+S"),
            "Toggle dark mode",
            "Zoom in",
            "About"
        ]
    );
}

#[test]
fn a_long_list_scrolls_to_keep_the_cursor_in_view() {
    let many = (0..40)
        .map(|i| Command::new(format!("c{i}"), format!("Command {i}")))
        .collect();
    let mut mounted = Mounted::with(many);
    mounted.open.set(true);
    mounted.settle();
    assert_eq!(mounted.pointed().as_deref(), Some("Command 0"));
    mounted.key(NamedKey::ArrowUp);
    assert_eq!(
        mounted.pointed().as_deref(),
        Some("Command 39"),
        "the last row was scrolled to and built"
    );
    let rows = ui_core::accessibility::snapshot(&mounted.tree.commands())
        .into_iter()
        .filter(|node| node.role == Role::MenuItem)
        .count();
    assert!(rows < 40, "only the rows on screen are built: {rows}");
}
