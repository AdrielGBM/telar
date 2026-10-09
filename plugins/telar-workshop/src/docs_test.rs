use std::cell::Cell;
use std::rc::Rc;

use telar::preview::{
    ControlKind, Layout, PreviewCtx, PreviewEntry, PropDefault, PropField, PropsSchema, SourceSpan,
};
use telar::testing::{centre, key_with, mount, press, release, route, texts};
use telar::{
    AccessNode, App, AppRuntime, Children, Color, ComponentList, Event, Key, LayoutError,
    LayoutItem, LayoutStyle, LocalApp, ModifiersState, Rect, Role, ScrollDelta, StyledContainer,
    Text, TextStyle, box_item,
};

use telar_components::{ButtonProps, button};

use super::{PAGE_MARGIN, paragraphs};
use crate::WorkshopApp;
use crate::test_support::{Drawn, control, drawn, settle};

const WIDTH: u32 = 1400;
const HEIGHT: u32 = 1100;
const DEFAULT: &str = "fake--badge--default";
const SECOND: &str = "fake--badge--second";
const TALL: &str = "fake--badge--tall";
const LAST: &str = "fake--badge--last";
const OTHER: &str = "fake--other--only";
const NOTED: &str = "fake--noted--only";
const TALL_LINES: usize = 30;

fn text_control() -> ControlKind {
    ControlKind::TEXT
}

fn count_control() -> ControlKind {
    ControlKind::INTEGER
}

static FIELDS: [PropField; 2] = [
    PropField::new("label", "Reactive<String>", text_control).doc("What the badge says."),
    PropField::new("count", "u32", count_control)
        .doc("How many it counts.")
        .default(PropDefault::Expr("3")),
];

static SCHEMA: PropsSchema = PropsSchema::new(
    "BadgeProps",
    " A badge counts things\n beside a label.\n\n It never takes focus.",
)
.fields(&FIELDS);

fn schema() -> &'static PropsSchema {
    &SCHEMA
}

static SPANS: [SourceSpan; 1] = [SourceSpan::new(0, 18, 42)];

thread_local! {
    static LAST_BUILDS: Cell<u32> = const { Cell::new(0) };
}

fn line(text: String) -> Result<Box<dyn LayoutItem>, LayoutError> {
    Ok(box_item(Text::new(
        move || text.clone(),
        LayoutStyle::new(),
        || TextStyle::new(13.0, Color::BLACK),
    )?))
}

fn default(ctx: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let label: String = ctx.arg("label", "Save".to_string());
    line(format!("label={label}"))
}

fn second(ctx: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let log = ctx.actions();
    button(
        ButtonProps::props()
            .label("second body")
            .on_press(Rc::new(move || log.log("on_second", Vec::new())))
            .build(),
        Children::default(),
    )
}

fn tall(_: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let lines = (1..=TALL_LINES)
        .map(|n| line(format!("tall line {n}")))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(box_item(StyledContainer::new(
        LayoutStyle::new().flex_column(),
        |_| Default::default(),
        lines,
    )?))
}

fn last(_: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    LAST_BUILDS.with(|builds| builds.set(builds.get() + 1));
    line("last body".to_string())
}

fn other(_: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    line("other body".to_string())
}

fn entries() -> Vec<PreviewEntry> {
    vec![
        PreviewEntry::new(DEFAULT, "badge", "Default", default)
            .title("Inputs/Badge")
            .props(schema)
            .source("badge(label: p.arg(\"label\", \"Save\"))", &SPANS),
        PreviewEntry::new(SECOND, "badge", "Second", second)
            .title("Inputs/Badge")
            .layout(Layout::Centered)
            .source("second_source()", &[]),
        PreviewEntry::new(TALL, "badge", "Tall", tall).title("Inputs/Badge"),
        PreviewEntry::new(LAST, "badge", "Last", last)
            .title("Inputs/Badge")
            .source("last_source()", &[]),
        PreviewEntry::new(OTHER, "other", "Only", other).title("Inputs/Other"),
        PreviewEntry::new(NOTED, "noted", "Only", other)
            .title("Inputs/Noted")
            .docs("Use a badge\nfor counts.\n\nNot for status.")
            .props(schema),
    ]
}

struct Docs {
    runtime: LocalApp<WorkshopApp>,
    tree: ComponentList,
}

impl Docs {
    fn open(id: &str) -> Self {
        telar::install_default_text_metrics();
        telar::hot_restore_json(r#"{"@workshop/view":"\"Docs\"","@workshop/panel.size":"160.0"}"#);
        let runtime = LocalApp(WorkshopApp::new(entries()).select(id));
        let tree = mount(runtime.0.root(), WIDTH, HEIGHT);
        let docs = Self { runtime, tree };
        settle(&docs.tree);
        docs
    }

    /// Whether the page draws `text`, as the whole of one string, leaving out the sidebar and the panels.
    fn draws(&self, text: &str) -> bool {
        let page = self.page();
        self.drawn()
            .iter()
            .any(|drawn| drawn.text == text && inside(drawn.rect, page))
    }

    /// The canvas area, as the box named for the view it shows.
    fn page(&self) -> Rect {
        self.access()
            .into_iter()
            .filter(|node| node.id.is_none() && node.name == "Docs")
            .map(|node| node.rect)
            .max_by(|a, b| (a.width * a.height).total_cmp(&(b.width * b.height)))
            .expect("the canvas area is named for the docs")
    }

    fn drawn(&self) -> Vec<Drawn> {
        drawn(&self.tree)
    }

    fn find(&self, needle: &str) -> Drawn {
        let page = self.page();
        self.drawn()
            .into_iter()
            .find(|drawn| drawn.text == needle && inside(drawn.rect, page))
            .unwrap_or_else(|| panic!("{needle} is not drawn"))
    }

    fn access(&self) -> Vec<AccessNode> {
        self.runtime.access_snapshot(&self.tree.commands())
    }

    fn control(&self, role: Role, name: &str) -> AccessNode {
        control(&self.access(), role, name)
    }

    fn dispatch(&mut self, event: Event) {
        route(&mut self.tree, &event);
        settle(&self.tree);
    }

    fn click(&mut self, role: Role, name: &str) {
        let rect = self.control(role, name).rect;
        self.press(rect);
    }

    fn press(&mut self, rect: Rect) {
        let (x, y) = centre(rect);
        self.dispatch(press(x, y));
        self.dispatch(release(x, y));
    }

    fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            self.dispatch(key_with(Key::Char(c), ModifiersState::default()));
        }
    }

    /// Turns the wheel `by` pixels down at `x`, `y`.
    fn wheel(&mut self, x: f32, y: f32, by: f32) {
        self.dispatch(Event::Scrolled {
            delta: ScrollDelta::Pixels { x: 0.0, y: -by },
            x: f64::from(x),
            y: f64::from(y),
        });
    }

    /// Turns the wheel over the page's own margin, where nothing nested in it can take the wheel, until `needle` is drawn, or gives up after `turns`.
    fn scroll_to(&mut self, needle: &str, turns: usize) -> bool {
        let over = self.page();
        let x = over.x + PAGE_MARGIN / 2.0;
        for _ in 0..turns {
            if self.draws(needle) {
                return true;
            }
            self.wheel(x, over.y + over.height / 2.0, 200.0);
        }
        self.draws(needle)
    }
}

#[test]
fn the_page_names_the_component_under_its_group() {
    let docs = Docs::open(DEFAULT);
    let group = docs.find("Inputs");
    let name = docs.find("Badge");
    assert!(group.rect.y < name.rect.y);
}

#[test]
fn the_prose_a_preview_carries_opens_the_page_ahead_of_the_props_doc() {
    let docs = Docs::open(NOTED);
    let name = docs.find("Noted");
    let prose = docs.find("Use a badge for counts.");
    let second = docs.find("Not for status.");
    let props = docs.find("A badge counts things beside a label.");
    assert!(name.rect.y < prose.rect.y);
    assert!(prose.rect.y < second.rect.y);
    assert!(second.rect.y < props.rect.y);
}

#[test]
fn a_component_whose_previews_carry_no_prose_opens_with_its_name() {
    let docs = Docs::open(OTHER);
    assert!(!docs.draws("Use a badge for counts."));
}

#[test]
fn the_props_doc_opens_the_page_as_paragraphs() {
    let docs = Docs::open(DEFAULT);
    assert!(docs.draws("A badge counts things beside a label."));
    assert!(docs.draws("It never takes focus."));
}

#[test]
fn the_page_lists_every_prop_with_its_type_default_and_doc() {
    let docs = Docs::open(DEFAULT);
    for text in [
        "Props",
        "Name",
        "Type",
        "Default",
        "Description",
        "label",
        "Reactive<String>",
        "Required",
        "What the badge says.",
        "count",
        "u32",
        "3",
        "How many it counts.",
    ] {
        assert!(docs.draws(text), "the props table draws {text}");
    }
}

#[test]
fn the_first_preview_is_mounted_with_its_controls() {
    let mut docs = Docs::open(SECOND);
    assert!(
        docs.draws("label=Save"),
        "the component's first preview, whichever is selected"
    );
    docs.click(Role::TextInput, "label");
    docs.type_text("!");
    assert!(docs.draws("label=Save!"));
}

#[test]
fn a_preview_shows_its_source_numbered_from_where_it_is_written() {
    let docs = Docs::open(DEFAULT);
    let source = docs.find("badge(label: p.arg(\"label\", \"Save\"))");
    assert!(source.rect.height > 0.0);
    assert!(docs.draws("42"), "the gutter starts at the span's line");
    assert!(docs.draws("second_source()"));
}

#[test]
fn only_the_selected_component_is_documented() {
    let mut docs = Docs::open(DEFAULT);
    assert!(docs.scroll_to("last_source()", 30));
    assert!(!docs.draws("other body"));
}

#[test]
fn a_preview_mounts_once_it_scrolls_into_view() {
    let mut docs = Docs::open(DEFAULT);
    assert!(
        docs.draws("second body"),
        "a preview in view mounts at once"
    );
    assert_eq!(
        LAST_BUILDS.with(Cell::get),
        0,
        "a preview below the fold waits"
    );
    assert!(docs.scroll_to("last body", 30));
    assert_eq!(LAST_BUILDS.with(Cell::get), 1);
}

#[test]
fn a_preview_gets_a_canvas_as_tall_as_what_it_draws() {
    let mut docs = Docs::open(DEFAULT);
    let last_line = format!("tall line {TALL_LINES}");
    assert!(docs.scroll_to(&last_line, 30));
    let first = docs.find("tall line 1");
    let last = docs.find(&last_line);
    let canvas = last.clip.expect("the preview is drawn inside its canvas");
    assert!(
        last.rect.y + last.rect.height <= canvas.y + canvas.height + 0.5,
        "the last line {:?} fits its canvas {canvas:?}",
        last.rect
    );
    assert!(last.rect.y > first.rect.y);
}

#[test]
fn a_wheel_over_a_source_that_only_scrolls_sideways_scrolls_the_page() {
    let mut docs = Docs::open(DEFAULT);
    let needle = "badge(label: p.arg(\"label\", \"Save\"))";
    let before = docs.find(needle).rect;
    docs.wheel(
        before.x + before.width / 2.0,
        before.y + before.height / 2.0,
        40.0,
    );
    let after = docs.find(needle).rect;
    assert!(
        (before.y - after.y - 40.0).abs() < 0.5,
        "the page moved the source up by the turn: {before:?} to {after:?}"
    );
}

#[test]
fn a_wheel_over_a_preview_s_canvas_scrolls_the_page() {
    let mut docs = Docs::open(DEFAULT);
    let before = docs.find("second body").rect;
    docs.wheel(
        before.x + before.width / 2.0,
        before.y + before.height / 2.0,
        40.0,
    );
    let after = docs.find("second body").rect;
    assert!(
        (before.y - after.y - 40.0).abs() < 0.5,
        "the page moved the preview up by the turn: {before:?} to {after:?}"
    );
}

#[test]
fn open_in_canvas_selects_the_preview_and_leaves_the_docs() {
    let mut docs = Docs::open(DEFAULT);
    docs.click(Role::Button, "Open in canvas: Second");
    let drawn = texts(&docs.tree);
    assert!(
        drawn.iter().any(|text| text == "Inputs/Badge / Second"),
        "the canvas header names it"
    );
    assert!(!drawn.iter().any(|text| text == "Props"));
}

#[test]
fn the_page_s_previews_log_their_calls_in_the_panels_beside_it() {
    let mut docs = Docs::open(DEFAULT);
    docs.click(Role::Tab, "Actions");
    let button = docs.find("second body").rect;
    docs.press(button);
    assert!(texts(&docs.tree).iter().any(|text| text == "on_second"));
}

#[test]
fn paragraphs_split_at_blank_lines_and_join_the_lines_between() {
    assert_eq!(
        paragraphs(" One line\n and the next.\n \n\n Another.\n"),
        ["One line and the next.", "Another."]
    );
    assert!(paragraphs("  \n ").is_empty());
}

fn inside(inner: Rect, outer: Rect) -> bool {
    const SLACK: f32 = 0.5;
    inner.x >= outer.x - SLACK
        && inner.y >= outer.y - SLACK
        && inner.x + inner.width <= outer.x + outer.width + SLACK
        && inner.y + inner.height <= outer.y + outer.height + SLACK
}
