use std::cell::Cell;

use telar::preview::{
    ArgSpec, ArgValue, ControlKind, PreviewCtx, PreviewEntry, PropDefault, PropField, PropsSchema,
};
use telar::testing::{mount, texts};
use telar::{
    AccessNode, App, AppRuntime, Color, ComponentList, Event, Key, LayoutError, LayoutItem,
    LayoutStyle, LocalApp, ModifiersState, NamedKey, PointerButton, PointerSource, Rect, Role,
    StyledContainer, Text, TextStyle, box_item, relayout_if_dirty,
};

use crate::WorkshopApp;

const WIDTH: u32 = 1600;
const HEIGHT: u32 = 1800;
const BADGE: &str = "fake--badge--all";
const LIVE: &str = "fake--badge--live";
const BARE: &str = "fake--badge--bare";

#[derive(Clone, Copy, Debug, PartialEq, telar::PreviewArg)]
enum Tone {
    Warm,
    Cool,
}

#[derive(Clone, Copy, Debug, PartialEq, telar::PreviewArg)]
enum Day {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
}

#[derive(Debug)]
struct Shape(u8);

fn text_control() -> ControlKind {
    ControlKind::TEXT
}

fn count_control() -> ControlKind {
    ControlKind::INTEGER.range(0.0, 10.0)
}

fn multiline() -> ControlKind {
    ControlKind::MULTILINE
}

static FIELDS: [PropField; 2] = [
    PropField::new("label", "Reactive<String>", text_control)
        .doc("The text on the badge.")
        .default(PropDefault::Expr("\"Save\"")),
    PropField::new("count", "u32", count_control).doc("How many it counts."),
];

static SCHEMA: PropsSchema = PropsSchema::new("BadgeProps", "A badge.").fields(&FIELDS);

fn schema() -> &'static PropsSchema {
    &SCHEMA
}

static DECLARED: [ArgSpec; 1] = [ArgSpec::new("notes").control(multiline)];

fn line(text: String) -> Result<Box<dyn LayoutItem>, LayoutError> {
    Ok(box_item(Text::new(
        move || text.clone(),
        LayoutStyle::new(),
        || TextStyle::new(13.0, Color::BLACK),
    )?))
}

fn column(lines: Vec<Box<dyn LayoutItem>>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    Ok(box_item(StyledContainer::new(
        LayoutStyle::new().flex_column(),
        |_| Default::default(),
        lines,
    )?))
}

fn badge(ctx: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let label: String = ctx.arg("label", "Save".to_string());
    let count: u32 = ctx.arg("count", 3);
    let size: f32 = ctx.arg("size", 1.5);
    let on: bool = ctx.arg("on", false);
    let tint: Color = ctx.arg("tint", Color::from_hex("#ff0000").unwrap());
    let tone: Tone = ctx.arg("tone", Tone::Warm);
    let day: Day = ctx.arg("day", Day::Monday);
    let note: Option<String> = ctx.arg("note", None);
    let notes: String = ctx.arg("notes", String::new());
    let shape = telar::__preview_arg!(ctx, "shape", Shape(3));
    column(vec![
        line(format!("label={label}"))?,
        line(format!("count={count}"))?,
        line(format!("size={size}"))?,
        line(format!("on={on}"))?,
        line(format!("tint={}", ArgValue::Color(tint)))?,
        line(format!("tone={tone:?}"))?,
        line(format!("day={day:?}"))?,
        line(format!("note={note:?}"))?,
        line(format!("notes={notes}"))?,
        line(format!("shape={}", shape.0))?,
    ])
}

thread_local! {
    static LIVE_BUILDS: Cell<u32> = const { Cell::new(0) };
}

fn live(ctx: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    LIVE_BUILDS.with(|builds| builds.set(builds.get() + 1));
    let on = ctx.signal("lit", false);
    Ok(box_item(Text::new(
        move || format!("lit={}", on.get()),
        LayoutStyle::new(),
        || TextStyle::new(13.0, Color::BLACK),
    )?))
}

fn bare(_: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    line("bare body".to_string())
}

fn entries() -> Vec<PreviewEntry> {
    vec![
        PreviewEntry::new(BADGE, "badge", "All", badge)
            .props(schema)
            .args(&DECLARED),
        PreviewEntry::new(LIVE, "badge", "Live", live),
        PreviewEntry::new(BARE, "badge", "Bare", bare),
    ]
}

struct Workshop {
    runtime: LocalApp<WorkshopApp>,
    tree: ComponentList,
}

impl Workshop {
    fn open(entries: Vec<PreviewEntry>, id: &str) -> Self {
        telar::install_default_text_metrics();
        telar::hot_restore_json(r#"{"@workshop/panel.size":"1000.0"}"#);
        let runtime = LocalApp(WorkshopApp::new(entries).select(id));
        let tree = mount(runtime.0.root(), WIDTH, HEIGHT);
        let mut workshop = Self { runtime, tree };
        workshop.settle();
        workshop
    }

    fn settle(&mut self) {
        for _ in 0..3 {
            relayout_if_dirty();
            let _ = self.tree.commands();
        }
    }

    fn draws(&self, text: &str) -> bool {
        texts(&self.tree).iter().any(|drawn| drawn == text)
    }

    fn access(&self) -> Vec<AccessNode> {
        self.runtime.access_snapshot(&self.tree.commands())
    }

    fn control(&self, role: Role, name: &str) -> AccessNode {
        let nodes = self.access();
        nodes
            .iter()
            .find(|node| node.id.is_some() && node.role == role && node.name == name)
            .cloned()
            .unwrap_or_else(|| {
                let controls: Vec<_> = nodes
                    .iter()
                    .filter(|node| node.id.is_some())
                    .map(|node| (node.role, node.name.as_str()))
                    .collect();
                panic!("no {role:?} named {name:?} among {controls:?}")
            })
    }

    fn dispatch(&mut self, event: Event) {
        telar::begin_batch();
        if !telar::dispatch_overlays(&event) {
            self.tree.on_event(&event);
        }
        telar::end_batch();
        self.settle();
    }

    fn click(&mut self, role: Role, name: &str) {
        let rect = self.control(role, name).rect;
        self.press_at(rect);
    }

    fn press_at(&mut self, rect: Rect) {
        let (x, y) = (
            f64::from(rect.x + rect.width / 2.0),
            f64::from(rect.y + rect.height / 2.0),
        );
        self.dispatch(Event::PointerPressed {
            x,
            y,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        });
        self.dispatch(Event::PointerReleased {
            x,
            y,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        });
    }

    fn key(&mut self, key: Key) {
        self.dispatch(Event::KeyPressed {
            key,
            modifiers: ModifiersState::default(),
        });
    }

    fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            self.key(Key::Char(c));
        }
    }

    /// Focuses the field, empties it and types `text` in.
    fn retype(&mut self, role: Role, name: &str, text: &str) {
        self.click(role, name);
        self.key(Key::Named(NamedKey::End));
        for _ in 0..16 {
            self.key(Key::Named(NamedKey::Backspace));
        }
        self.type_text(text);
    }
}

#[test]
fn every_arg_gets_a_row_and_the_preview_its_defaults() {
    let workshop = Workshop::open(entries(), BADGE);
    for text in [
        "label=Save",
        "count=3",
        "size=1.5",
        "on=false",
        "tint=#ff0000",
        "tone=Warm",
        "day=Monday",
        "note=None",
        "notes=",
        "shape=3",
    ] {
        assert!(workshop.draws(text), "the preview draws {text}");
    }
    for name in [
        "label", "count", "size", "on", "tint", "tone", "day", "note", "notes", "shape",
    ] {
        assert!(workshop.draws(name), "a row names {name}");
    }
    for heading in ["Controls", "Name", "Value", "Default", "Description"] {
        assert!(workshop.draws(heading), "the panel draws {heading}");
    }
}

#[test]
fn a_linked_prop_shows_its_type_default_and_doc() {
    let workshop = Workshop::open(entries(), BADGE);
    for text in [
        "Reactive<String>",
        "\"Save\"",
        "The text on the badge.",
        "u32",
        "How many it counts.",
    ] {
        assert!(workshop.draws(text), "the panel draws {text}");
    }
}

#[test]
fn an_arg_without_a_prop_shows_what_its_control_says_of_it() {
    let workshop = Workshop::open(entries(), BADGE);
    for text in [
        "bool",
        "float",
        "Color",
        "Warm | Cool",
        "Option<String>",
        "1.5",
        "false",
    ] {
        assert!(workshop.draws(text), "the panel draws {text}");
    }
}

#[test]
fn a_read_only_row_shows_its_value() {
    let workshop = Workshop::open(entries(), BADGE);
    assert!(workshop.draws("Shape(3)"));
}

#[test]
fn every_control_is_named_by_its_arg() {
    let workshop = Workshop::open(entries(), BADGE);
    workshop.control(Role::Switch, "on");
    workshop.control(Role::TextInput, "label");
    workshop.control(Role::Slider, "count");
    workshop.control(Role::TextInput, "tint");
    workshop.control(Role::MultilineTextInput, "notes");
    workshop.control(Role::Switch, "Unset note");
    let nodes = workshop.access();
    let named = |role: Role, name: &str| {
        nodes
            .iter()
            .any(|node| node.role == role && node.name.starts_with(name))
    };
    assert!(named(Role::SpinButton, "size"), "{nodes:#?}");
    assert!(named(Role::ComboBox, "day"), "{nodes:#?}");
}

#[test]
fn a_toggle_sets_its_arg_and_the_preview_builds_again() {
    let mut workshop = Workshop::open(entries(), BADGE);
    workshop.click(Role::Switch, "on");
    assert!(workshop.draws("on=true"));
}

#[test]
fn a_text_field_sets_its_arg_as_it_is_typed() {
    let mut workshop = Workshop::open(entries(), BADGE);
    workshop.click(Role::TextInput, "label");
    workshop.key(Key::Named(NamedKey::End));
    workshop.type_text("!");
    assert!(workshop.draws("label=Save!"));
}

#[test]
fn a_multiline_field_sets_its_arg() {
    let mut workshop = Workshop::open(entries(), BADGE);
    workshop.click(Role::MultilineTextInput, "notes");
    workshop.type_text("hi");
    assert!(workshop.draws("notes=hi"));
}

#[test]
fn a_number_field_steps_its_arg() {
    let mut workshop = Workshop::open(entries(), BADGE);
    let size = workshop
        .access()
        .into_iter()
        .find(|node| node.role == Role::SpinButton && node.name.starts_with("size"))
        .expect("the size field is listed");
    workshop.click(Role::SpinButton, &size.name);
    workshop.key(Key::Named(NamedKey::ArrowRight));
    assert!(workshop.draws("size=3"));
}

#[test]
fn a_ranged_number_is_a_slider() {
    let mut workshop = Workshop::open(entries(), BADGE);
    let count = workshop.control(Role::Slider, "count");
    assert_eq!(
        count.value.map(|value| (value.min, value.max)),
        Some((0.0, 10.0))
    );
    workshop.click(Role::Slider, "count");
    workshop.key(Key::Named(NamedKey::ArrowRight));
    assert!(
        workshop.draws("count=6"),
        "a press at the middle sets 5, and → steps once"
    );
}

#[test]
fn a_colour_field_sets_its_arg_once_it_reads_as_one() {
    let mut workshop = Workshop::open(entries(), BADGE);
    workshop.retype(Role::TextInput, "tint", "#00ff00");
    assert!(workshop.draws("tint=#00ff00"));
}

#[test]
fn a_short_choice_is_segmented() {
    let mut workshop = Workshop::open(entries(), BADGE);
    workshop.click(Role::Tab, "Cool");
    assert!(workshop.draws("tone=Cool"));
}

#[test]
fn a_long_choice_is_a_select() {
    let mut workshop = Workshop::open(entries(), BADGE);
    let day = workshop
        .access()
        .into_iter()
        .find(|node| node.role == Role::ComboBox && node.name.starts_with("day"))
        .expect("the day select is listed");
    workshop.click(Role::ComboBox, &day.name);
    workshop.click(Role::MenuItem, "Friday");
    assert!(workshop.draws("day=Friday"));
}

#[test]
fn an_optional_arg_is_set_by_its_control_and_unset_by_its_switch() {
    let mut workshop = Workshop::open(entries(), BADGE);
    assert_eq!(
        workshop.control(Role::Switch, "Unset note").toggled,
        Some(true)
    );
    workshop.click(Role::TextInput, "note");
    workshop.type_text("hi");
    assert!(workshop.draws("note=Some(\"hi\")"));
    assert_eq!(
        workshop.control(Role::Switch, "Unset note").toggled,
        Some(false)
    );
    workshop.click(Role::Switch, "Unset note");
    assert!(workshop.draws("note=None"));
    workshop.click(Role::Switch, "Unset note");
    assert!(
        workshop.draws("note=Some(\"hi\")"),
        "setting it again restores what the control shows"
    );
}

#[test]
fn a_live_arg_reaches_the_preview_without_building_it_again() {
    let mut workshop = Workshop::open(entries(), LIVE);
    let builds = LIVE_BUILDS.with(Cell::get);
    workshop.click(Role::Switch, "lit");
    assert!(workshop.draws("lit=true"));
    assert_eq!(LIVE_BUILDS.with(Cell::get), builds);
}

#[test]
fn an_edited_arg_offers_its_own_reset() {
    let mut workshop = Workshop::open(entries(), BADGE);
    assert!(!workshop.draws("Reset"), "nothing to reset yet");
    workshop.click(Role::Switch, "on");
    workshop.click(Role::TextInput, "label");
    workshop.type_text("x");
    workshop.click(Role::Button, "Reset label");
    assert!(workshop.draws("label=Save"));
    assert!(workshop.draws("on=true"), "only its own arg goes back");
    workshop.control(Role::Button, "Reset on");
    assert!(
        !workshop
            .access()
            .iter()
            .any(|node| node.name == "Reset label"),
        "a reset arg offers no reset"
    );
}

#[test]
fn reset_all_puts_every_arg_back() {
    let mut workshop = Workshop::open(entries(), BADGE);
    assert!(!workshop.draws("Reset all"));
    workshop.click(Role::Switch, "on");
    workshop.click(Role::Tab, "Cool");
    workshop.click(Role::Button, "Reset all");
    assert!(workshop.draws("on=false"));
    assert!(workshop.draws("tone=Warm"));
    assert!(!workshop.draws("Reset all"));
    assert_eq!(workshop.control(Role::Switch, "on").toggled, Some(false));
}

#[test]
fn edited_args_survive_a_hot_reload() {
    let mut workshop = Workshop::open(entries(), BADGE);
    workshop.click(Role::Switch, "on");
    let snapshot = telar::hot_snapshot_json();
    telar::hot_restore_json(&snapshot);
    let workshop = Workshop::open(entries(), BADGE);
    assert!(workshop.draws("on=true"));
    assert_eq!(workshop.control(Role::Switch, "on").toggled, Some(true));
    workshop.control(Role::Button, "Reset on");
}

#[test]
fn a_preview_without_args_says_so() {
    let workshop = Workshop::open(entries(), BARE);
    assert!(workshop.draws("No args"));
}

#[test]
fn nothing_selected_says_so() {
    let workshop = Workshop::open(Vec::new(), BADGE);
    assert!(workshop.draws("Nothing selected"));
}
