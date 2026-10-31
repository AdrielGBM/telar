use std::cell::RefCell;

use telar::testing::{centre, key_with, named, press, release, route, texts};
use telar::{
    AvailableSpace, ComponentList, Event, compute_layout, focus, new_container, relayout_if_dirty,
};

use super::*;
use crate::shared::row_height;

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
        Self::in_window(commands, 600.0)
    }

    fn in_window(commands: Vec<Command>, height: f32) -> Self {
        Self::in_size(commands, 800.0, height)
    }

    fn in_size(commands: Vec<Command>, width: f32, height: f32) -> Self {
        crate::test_support::fresh_layout_runtime();
        telar::set_locale("en");
        telar::testing::reset_keyboard();
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
        )
        .unwrap();
        let root = new_container(
            LayoutStyle::new().flex_column().width(width).height(height),
            &[palette.layout_node()],
        )
        .unwrap();
        compute_layout(
            root,
            AvailableSpace::Definite(width),
            AvailableSpace::Definite(height),
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
        let nodes = telar::inspect::snapshot(&self.tree.commands());
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
    let save = telar::inspect::snapshot(&mounted.tree.commands())
        .into_iter()
        .find(|node| node.role == Role::Option && node.name.starts_with("Save"))
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
    let nodes = telar::inspect::snapshot(&mounted.tree.commands());
    assert!(
        nodes
            .iter()
            .any(|node| node.role == Role::Dialog && node.name == "Command palette"),
        "the dialog is named"
    );
    let rows: Vec<&str> = nodes
        .iter()
        .filter(|node| node.role == Role::Option)
        .map(|node| node.name.as_str())
        .collect();
    assert_eq!(
        rows,
        [
            "Open file, Ctrl+O",
            "Save, Ctrl+S",
            "Toggle dark mode",
            "Zoom in",
            "About"
        ]
    );
    let keys: Vec<Option<&str>> = nodes
        .iter()
        .filter(|node| node.role == Role::Option)
        .map(|node| node.keyboard_shortcut.as_deref())
        .collect();
    assert_eq!(
        keys,
        [Some("Control+O"), Some("Control+S"), None, None, None],
        "and is told the keys of a row that has them"
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
    let rows = telar::inspect::snapshot(&mounted.tree.commands())
        .into_iter()
        .filter(|node| node.role == Role::Option)
        .count();
    assert!(rows < 40, "only the rows on screen are built: {rows}");
}

fn numbered(count: usize) -> Vec<Command> {
    (0..count)
        .map(|i| Command::new(format!("c{i}"), format!("Command {i}")))
        .collect()
}

fn bottom(rect: telar::Rect) -> f32 {
    rect.y + rect.height
}

impl Mounted {
    fn rect_of(&self, role: Role, name: Option<&str>) -> telar::Rect {
        telar::inspect::snapshot(&self.tree.commands())
            .into_iter()
            .find(|node| node.role == role && name.is_none_or(|name| node.name == name))
            .unwrap_or_else(|| panic!("a {role:?} named {name:?} is drawn"))
            .rect
    }

    /// Whether the list holds focus, and the name of the row it points a reader at.
    fn list_pointing(&self) -> (bool, Option<String>) {
        let nodes = telar::inspect::snapshot(&self.tree.commands());
        let list = nodes
            .iter()
            .find(|node| node.role == Role::ListBox)
            .expect("the list is drawn");
        let pointed = list.active_descendant.and_then(|row| {
            nodes
                .iter()
                .find(|node| node.id == Some(row))
                .map(|node| node.name.clone())
        });
        (list.focused, pointed)
    }
}

#[test]
fn a_short_list_is_as_tall_as_its_rows() {
    let mounted = Mounted::opened();
    assert_eq!(
        mounted.rect_of(Role::ListBox, None).height,
        7.0 * row_height()
    );
}

/// Eight rows under a field, below the offset the dialog keeps from the top, are more than a short window holds: the list used to run out of the bottom of the surface.
#[test]
fn in_a_short_window_the_list_gives_up_rows_to_fit_and_keeps_the_cursor_in_view() {
    let height = 300.0;
    let mut mounted = Mounted::in_window(numbered(40), height);
    mounted.open.set(true);
    mounted.settle();
    let card = mounted.rect_of(Role::Dialog, None);
    let list = mounted.rect_of(Role::ListBox, None);
    assert!(bottom(card) <= height, "the dialog fits: {card:?}");
    assert!(
        list.height < VISIBLE_ROWS as f32 * row_height(),
        "the list gave up rows: {list:?}"
    );
    assert!(
        list.height >= row_height(),
        "and still shows some: {list:?}"
    );

    mounted.key(NamedKey::ArrowUp);
    let last = mounted.rect_of(Role::Option, Some("Command 39"));
    assert!(
        last.y >= list.y - 0.5 && bottom(last) <= bottom(list) + 0.5,
        "the cursor's row is in view: {last:?} in {list:?}"
    );
}

/// The list scrolls, so it is a stop of its own: the one control a reader meets for it, walked by the cursor the field walks.
#[test]
fn the_list_is_one_stop_that_walks_the_cursor_the_field_does() {
    let mut mounted = Mounted::opened();
    mounted.key(NamedKey::Tab);
    let (focused, pointed) = mounted.list_pointing();
    assert!(focused, "Tab moves from the field to the list");
    assert!(pointed.is_some_and(|name| name.starts_with("Open file")));

    mounted.key(NamedKey::ArrowDown);
    let (_, pointed) = mounted.list_pointing();
    assert!(
        pointed
            .as_deref()
            .is_some_and(|name| name.starts_with("Save")),
        "{pointed:?}"
    );
    mounted.key(NamedKey::Enter);
    assert_eq!(*mounted.ran.borrow(), ["save"]);
}

/// The field names the list of results as the one it controls, so a reader knows where the row it points at is, and the document can find that row there.
#[test]
fn the_field_names_the_list_of_results_it_controls() {
    let mounted = Mounted::opened();
    let nodes = telar::inspect::snapshot(&mounted.tree.commands());
    let field = nodes
        .iter()
        .find(|node| node.role == Role::TextInput && node.focused)
        .expect("the field holds the keyboard");
    let list = nodes
        .iter()
        .find(|node| node.role == Role::ListBox)
        .expect("the results are one list");
    assert_eq!(field.controls, list.id);
}

/// The results are a named list box, and the option the cursor is on is the selected one, as the list autocomplete pattern has it: the one Enter runs.
#[test]
fn the_results_are_a_named_list_box_whose_cursor_row_is_selected() {
    let mut mounted = Mounted::opened();
    let selected = |mounted: &Mounted| -> Vec<String> {
        telar::inspect::snapshot(&mounted.tree.commands())
            .into_iter()
            .filter(|node| node.role == Role::Option && node.toggled == Some(true))
            .map(|node| node.name)
            .collect()
    };
    let nodes = telar::inspect::snapshot(&mounted.tree.commands());
    let list = nodes
        .iter()
        .find(|node| node.role == Role::ListBox)
        .expect("the results are one list");
    assert_eq!(list.name, "Commands");
    assert!(
        nodes
            .iter()
            .all(|node| node.role != Role::Menu && node.role != Role::MenuItem),
        "nothing in the palette is announced as a menu"
    );
    assert_eq!(
        selected(&mounted),
        [mounted.pointed().expect("a cursor row")]
    );

    mounted.key(NamedKey::ArrowDown);
    assert_eq!(
        selected(&mounted),
        [mounted.pointed().expect("a cursor row")]
    );
}

#[derive(Clone)]
struct Themed {
    palettes: Option<CommandPaletteTokens>,
}

impl telar::ThemeTokens for Themed {
    fn radius(&self) -> f32 {
        4.0
    }
    fn register_extensions(&self, extensions: &mut telar::ThemeExtensions) {
        if let Some(palettes) = self.palettes {
            extensions.insert(palettes);
        }
    }
}

/// An open palette's dialog, as it is drawn and as it is painted.
fn dialog(style: Option<telar::SurfaceAmendment>) -> (telar::Rect, RectStyle) {
    crate::test_support::fresh_layout_runtime();
    let open = signal(false);
    let props = CommandPaletteProps::props()
        .open(open)
        .commands(catalogue());
    let props = match style {
        Some(style) => props.style(style),
        None => props,
    };
    let tree = telar::testing::scoped(|| {
        let palette = command_palette(props.build()).unwrap();
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
        ComponentList::new(palette)
    });
    let _ = tree.commands();
    open.set(true);
    relayout_if_dirty();
    let _ = tree.commands();
    relayout_if_dirty();
    let rect = telar::inspect::snapshot(&tree.commands())
        .into_iter()
        .find(|node| node.role == Role::Dialog)
        .expect("the dialog is drawn")
        .rect;
    let style = tree
        .commands()
        .iter()
        .find_map(|c| match c {
            telar::DrawCommand::Rect { rect: at, style } if *at == rect => Some(**style),
            _ => None,
        })
        .expect("the dialog is painted");
    (rect, style)
}

#[test]
fn the_dialog_takes_the_dialog_corner_until_the_palette_tokens_name_their_own() {
    use telar::{BorderRadius, Paint, ThemeTokens};

    telar::set_theme(Themed { palettes: None });
    let (rect, style) = dialog(None);
    assert_eq!(
        style.radius,
        BorderRadius::all(Themed { palettes: None }.radius_dialog())
    );
    assert_eq!(rect.width, CommandPaletteTokens::default().width());

    telar::set_theme(Themed {
        palettes: Some(CommandPaletteTokens {
            radius: Some(5.0),
            width: Some(300.0),
            ..CommandPaletteTokens::default()
        }),
    });
    let green = Color::rgba(0.1, 0.6, 0.2, 1.0);
    let (rect, style) = dialog(Some(Rc::new(move |style: RectStyle| {
        style.with_fill(green)
    })));
    assert_eq!(style.radius, BorderRadius::all(5.0));
    assert_eq!(rect.width, 300.0);
    assert_eq!(
        style.fill,
        Some(Paint::Solid(green)),
        "the caller's amendment comes last"
    );
}

#[test]
fn in_a_narrow_window_the_dialog_stays_inside_it() {
    for width in [200.0, 320.0, 480.0] {
        let mut mounted = Mounted::in_size(numbered(40), width, 600.0);
        mounted.open.set(true);
        mounted.settle();
        let card = mounted.rect_of(Role::Dialog, None);
        assert!(
            card.x >= 0.0 && card.x + card.width <= width,
            "inside {width}: {card:?}"
        );
        let field = mounted.rect_of(Role::TextInput, None);
        assert!(
            field.x >= card.x && field.x + field.width <= card.x + card.width + 0.5,
            "the field stays in the dialog at {width}: {field:?} in {card:?}"
        );
    }
}
