use std::rc::Rc;

use telar::preview::snapshot::SnapshotStatus;
use telar::preview::{
    ArgSpec, ArgValue, Axis, ControlKind, Globals, Matrices, Matrix, Play, PreviewCtx,
    PreviewEntry, PreviewEnv, ViewportPreset,
};
use telar::testing::{Scoped, centre, mount, named as named_key, press, release, route, texts};
use telar::{
    AccessNode, App, AppRuntime, Children, Color, ColorScheme, ComponentList, ControlSize,
    Direction, DrawCommand, Event, LayoutError, LayoutItem, LayoutStyle, LocalApp, NamedKey, Rect,
    Role, Size, SystemPreferences, Text, TextStyle, box_item, focus, for_each_with_matrix,
    reset_layout_runtime, set_system_preferences, transform_clip_rect,
};
use telar_components::{ButtonProps, ModalProps, button, modal};

use super::cell::{Status, seed, sizing};
use super::plan::{self, CustomAxis, MatrixAxes, PickedAxis, Problem, grid, toggled};
use super::*;
use crate::settings::{Background, CanvasSettings, ModeChoice};
use crate::state::{PanelTab, WorkshopState};
use crate::test_support::{Shell, settle};

const WIDTH: u32 = 1600;
const HEIGHT: u32 = 1100;
const DAY: Color = Color::rgb(0.95, 0.9, 0.8);
const NIGHT: Color = Color::rgb(0.05, 0.1, 0.2);
const AXES_MENU: &str = "Pick the matrix's axes";

#[derive(Clone)]
struct Day;

impl telar::ThemeTokens for Day {
    fn surface(&self) -> Color {
        DAY
    }
}

#[derive(Clone)]
struct Night;

impl telar::ThemeTokens for Night {
    fn surface(&self) -> Color {
        NIGHT
    }
}

fn register_modes() {
    telar::register_mode_theme("day", Day);
    telar::register_mode_theme("night", Night);
}

fn widget(_: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    Ok(box_item(Text::new(
        || "widget".to_string(),
        LayoutStyle::new(),
        || TextStyle::new(13.0, Color::BLACK),
    )?))
}

/// A widget lit or not by an arg it reads as it builds, and declares nowhere else.
fn lit(ctx: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let on = ctx.signal("lit", false);
    Ok(box_item(Text::new(
        move || format!("lit {}", on.get()),
        LayoutStyle::new(),
        || TextStyle::new(13.0, Color::BLACK),
    )?))
}

/// A button that opens the dialog named `confirm`, and the dialog.
fn confirming(_: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let opener = button(
        ButtonProps::props()
            .label("Ask")
            .on_press(Rc::new(|| telar::open_overlay("confirm")))
            .build(),
        Children::default(),
    )?;
    let dialog = modal(
        ModalProps::props().id("confirm").title("Sure?").build(),
        Children::default(),
    )?;
    Ok(box_item(telar::Container::new(
        LayoutStyle::new().flex_column(),
        vec![opener, dialog],
    )?))
}

fn counter(_: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let presses = telar::signal(0_u32);
    button(
        ButtonProps::props()
            .label(telar::Reactive::of(move || {
                format!("Pressed {}", presses.get())
            }))
            .on_press(Rc::new(move || presses.update(|count| *count += 1)))
            .build(),
        Children::default(),
    )
}

fn toggle_control() -> ControlKind {
    ControlKind::Toggle
}

static GHOST: [ArgSpec; 1] = [ArgSpec::new("ghost")
    .control(toggle_control)
    .default("false")];

const NINE: &[&str] = &["1", "2", "3", "4", "5", "6", "7", "8", "9"];
const SEVEN: &[&str] = &["1", "2", "3", "4", "5", "6", "7"];

const THEMED: &str = "kit--badge--themed";
const BOTH_WAYS: &str = "kit--badge--both-ways";
const PLAIN: &str = "kit--badge--plain";
const FAILING: &str = "kit--badge--failing";
const ASKING: &str = "kit--dialog--asking";
const COUNTING: &str = "kit--counter--counting";
const LONG_FAILING: &str = "kit--badge--long-failing";
const LIT: &str = "kit--badge--lit";

fn entries() -> Vec<PreviewEntry> {
    vec![
        PreviewEntry::new(THEMED, "badge", "Themed", widget).matrix(Matrix::Named("themes")),
        PreviewEntry::new(BOTH_WAYS, "badge", "Both ways", widget)
            .matrix(Matrix::Axes(&[Axis::Dir(&[
                Direction::Ltr,
                Direction::Rtl,
            ])]))
            .args(&GHOST)
            .play(|canvas| canvas.expect_text("widget")),
        PreviewEntry::new(PLAIN, "badge", "Plain", widget).args(&GHOST),
        PreviewEntry::new(FAILING, "badge", "Failing", widget)
            .matrix(Matrix::Axes(&[Axis::Dir(&[Direction::Ltr])]))
            .play(|canvas| canvas.expect_text("gadget")),
        PreviewEntry::new(LIT, "badge", "Lit", lit),
        PreviewEntry::new(LONG_FAILING, "badge", "Long failing", widget)
            .matrix(Matrix::Axes(&[Axis::Dir(&[Direction::Ltr])]))
            .play(|canvas| {
                canvas.expect_text(
                    "a text long enough that the message saying it is missing runs well past one cell",
                )
            }),
        PreviewEntry::new(COUNTING, "counter", "Counting", counter)
            .matrix(Matrix::Axes(&[Axis::Dir(&[Direction::Ltr])]))
            .play(|canvas| {
                canvas.click(telar::preview::by_role(Role::Button).named("Pressed 0"))?;
                canvas.expect_text("Pressed 1")
            }),
        PreviewEntry::new(ASKING, "dialog", "Asking", confirming).matrix(Matrix::Axes(&[
            Axis::Dir(&[Direction::Ltr, Direction::Rtl]),
        ])),
    ]
}

struct Workshop {
    runtime: LocalApp<Shell>,
    tree: Scoped<ComponentList>,
}

impl Workshop {
    fn open(id: &str) -> Self {
        reset_layout_runtime();
        focus::clear();
        telar::install_default_text_metrics();
        let request = telar::preview::host::PreviewRequest::new(Some(id), None);
        let state = WorkshopState::new(entries().into(), &request);
        state.view().set(ViewMode::Matrix);
        let runtime = LocalApp(Shell { state });
        let tree = mount(|| runtime.0.root(), WIDTH, HEIGHT);
        let workshop = Self { runtime, tree };
        settle(&workshop.tree);
        workshop.next_turn();
        workshop
    }

    fn state(&self) -> &WorkshopState {
        &self.runtime.0.state
    }

    fn settle(&self) {
        settle(&self.tree);
    }

    fn next_turn(&self) {
        telar::testing::advance_time(std::time::Duration::ZERO);
        self.settle();
    }

    fn send(&mut self, event: Event) {
        route(&mut self.tree, &event);
        self.settle();
    }

    fn nodes(&self) -> Vec<AccessNode> {
        self.runtime.access_snapshot(&self.tree.commands())
    }

    fn texts(&self) -> Vec<String> {
        texts(&self.tree)
    }

    fn draws(&self, text: &str) -> bool {
        self.texts().iter().any(|drawn| drawn == text)
    }

    fn count(&self, text: &str) -> usize {
        self.texts().iter().filter(|drawn| *drawn == text).count()
    }

    fn node(&self, role: Role, name: &str) -> AccessNode {
        self.nodes()
            .into_iter()
            .find(|node| node.role == role && node.name == name)
            .unwrap_or_else(|| {
                let named: Vec<_> = self
                    .nodes()
                    .into_iter()
                    .map(|node| (node.role, node.name))
                    .collect();
                panic!("no {role:?} named {name:?} among {named:?}")
            })
    }

    fn click(&mut self, role: Role, name: &str) {
        let (x, y) = centre(self.node(role, name).rect);
        self.send(press(x, y));
        self.send(release(x, y));
    }

    /// Turns `axis` on or off from the header's menu of axes, which stays open for it, and shuts the menu after.
    fn pick(&mut self, axis: &str) {
        self.click(Role::Button, AXES_MENU);
        let (x, y) = centre(self.axis_row(axis).rect);
        self.send(press(x, y));
        self.send(release(x, y));
        self.send(named_key(NamedKey::Escape));
    }

    /// The row of the open menu of axes naming `axis`, which reads out the values it takes after it.
    fn axis_row(&self, axis: &str) -> AccessNode {
        let named = format!("{axis} ");
        self.nodes()
            .into_iter()
            .find(|node| node.role == Role::MenuItemCheckBox && node.name.starts_with(&named))
            .unwrap_or_else(|| panic!("no row for {axis} in {:?}", self.nodes()))
    }

    /// What `axis` takes, as its row in the menu of axes says.
    fn values(&mut self, axis: &str) -> String {
        self.click(Role::Button, AXES_MENU);
        let name = self.axis_row(axis).name;
        self.send(named_key(NamedKey::Escape));
        name[axis.len() + 1..].to_string()
    }

    /// Whether `axis` is on in the header's menu of axes.
    fn picked(&mut self, axis: &str) -> Option<bool> {
        self.click(Role::Button, AXES_MENU);
        let on = self.axis_row(axis).toggled;
        self.send(named_key(NamedKey::Escape));
        on
    }

    fn reset_axes(&mut self) {
        self.click(Role::Button, AXES_MENU);
        self.click(Role::MenuItem, "Reset the axes");
    }

    fn secondary_click(&mut self, role: Role, name: &str) {
        let (x, y) = centre(self.node(role, name).rect);
        let button = telar::PointerButton::Secondary;
        self.send(Event::PointerPressed {
            x,
            y,
            button,
            source: telar::PointerSource::Mouse,
        });
        self.send(Event::PointerReleased {
            x,
            y,
            button,
            source: telar::PointerSource::Mouse,
        });
    }

    /// The cells shown, by the name of the control that focuses each.
    fn cells(&self) -> Vec<String> {
        self.nodes()
            .into_iter()
            .filter(|node| node.role == Role::Button && node.name.starts_with("Focus "))
            .map(|node| node.name)
            .collect()
    }

    /// Each rect filled with `color`, where the window draws it.
    fn fills(&self, color: Color) -> Vec<Rect> {
        let mut found = Vec::new();
        for_each_with_matrix(&self.tree.commands(), |command, matrix| {
            if let DrawCommand::Rect { rect, style } = command
                && style.fill == Some(telar::Paint::Solid(color))
            {
                found.push(transform_clip_rect(matrix, *rect));
            }
        });
        found
    }
}

#[test]
fn the_themes_matrix_shows_the_preview_in_every_registered_mode_side_by_side() {
    register_modes();
    let workshop = Workshop::open(THEMED);
    assert_eq!(workshop.cells(), ["Focus mode: day", "Focus mode: night"]);
    assert!(workshop.draws("mode: day") && workshop.draws("mode: night"));
    assert_eq!(workshop.count("widget"), 2, "{:?}", workshop.texts());
    let (day, night) = (workshop.fills(DAY), workshop.fills(NIGHT));
    let (Some(day), Some(night)) = (day.first(), night.first()) else {
        panic!("each cell's canvas takes its mode's theme: {day:?} {night:?}");
    };
    assert!(night.x > day.x + day.width - 1.0, "{day:?} then {night:?}");
    assert!((night.y - day.y).abs() < 1.0, "on one row");
}

#[test]
fn the_cells_follow_the_system_scheme_as_it_changes() {
    register_modes();
    telar::set_mode_scheme("day", ColorScheme::Light);
    telar::set_mode_scheme("night", ColorScheme::Dark);
    let workshop = Workshop::open(BOTH_WAYS);
    assert_eq!(workshop.fills(DAY).len(), 2, "{:?}", workshop.texts());
    assert!(workshop.fills(NIGHT).is_empty());

    set_system_preferences(SystemPreferences {
        color_scheme: Some(ColorScheme::Dark),
        ..SystemPreferences::default()
    });
    workshop.next_turn();
    assert_eq!(
        workshop.fills(NIGHT).len(),
        2,
        "each cell is mounted again in the dark mode"
    );
    assert!(workshop.fills(DAY).is_empty());
}

#[test]
fn each_cell_runs_its_play_and_checks_what_the_play_left() {
    let workshop = Workshop::open(BOTH_WAYS);
    assert_eq!(workshop.cells(), ["Focus dir: ltr", "Focus dir: rtl"]);
    assert_eq!(workshop.count("Play passed"), 2, "{:?}", workshop.texts());
    let checked = workshop.count("Accessible")
        + workshop
            .texts()
            .iter()
            .filter(|text| text.starts_with("A11y"))
            .count();
    assert_eq!(checked, 2, "each cell's accessibility is checked");
    drop(workshop);

    let failing = Workshop::open(FAILING);
    assert!(failing.draws("Play failed"), "{:?}", failing.texts());
    assert!(
        failing
            .texts()
            .iter()
            .any(|text| text.contains("\"gadget\"")),
        "the cell says why: {:?}",
        failing.texts()
    );
}

#[test]
fn a_preview_with_no_matrix_builds_one_from_the_axes_picked() {
    let mut workshop = Workshop::open(PLAIN);
    assert!(workshop.draws("This preview has no matrix. Pick axes to build one."));
    assert!(workshop.cells().is_empty());

    workshop.pick("Direction");
    assert_eq!(
        workshop.state().matrix_axes().peek(),
        MatrixAxes::Picked(vec![PickedAxis::all_of(CustomAxis::Direction)])
    );
    assert_eq!(workshop.cells(), ["Focus dir: ltr", "Focus dir: rtl"]);

    workshop.pick("ghost");
    assert_eq!(
        workshop.cells(),
        [
            "Focus dir: ltr · ghost: false",
            "Focus dir: ltr · ghost: true",
            "Focus dir: rtl · ghost: false",
            "Focus dir: rtl · ghost: true",
        ]
    );
    for heading in ["ghost: false", "ghost: true", "dir: ltr", "dir: rtl"] {
        assert!(workshop.draws(heading), "{heading} heads a row or column");
    }
}

/// The preview's own matrix is where the axes start: picking another changes it into a matrix of the axes picked, those it had keeping its values, and a button brings it back.
#[test]
fn picking_an_axis_changes_the_preview_s_matrix_and_restoring_brings_it_back() {
    let mut workshop = Workshop::open(BOTH_WAYS);
    assert_eq!(workshop.picked("Direction"), Some(true));
    assert_eq!(workshop.state().matrix_axes().peek(), MatrixAxes::Declared);

    workshop.pick("ghost");
    assert_eq!(
        workshop.state().matrix_axes().peek(),
        MatrixAxes::Picked(vec![
            PickedAxis {
                axis: CustomAxis::Direction,
                declared: true,
            },
            PickedAxis::all_of(CustomAxis::Arg("ghost".into())),
        ])
    );
    assert_eq!(workshop.cells().len(), 4);

    workshop.pick("ghost");
    assert_eq!(
        workshop.state().matrix_axes().peek(),
        MatrixAxes::Declared,
        "back to the axes the preview declares is its own matrix"
    );

    workshop.pick("Direction");
    assert!(workshop.draws("Pick axes to build a matrix."));
    workshop.reset_axes();
    assert_eq!(workshop.cells(), ["Focus dir: ltr", "Focus dir: rtl"]);
}

/// An axis the preview's own matrix narrows keeps those values while it stays picked, and takes every value once it is picked again.
#[test]
fn an_axis_picked_again_takes_every_value() {
    let mut workshop = Workshop::open(COUNTING);
    assert_eq!(workshop.cells(), ["Focus dir: ltr"]);
    assert_eq!(workshop.values("Direction"), "ltr");
    workshop.pick("Direction");
    workshop.pick("Direction");
    assert_eq!(workshop.cells(), ["Focus dir: ltr", "Focus dir: rtl"]);
    assert_eq!(workshop.values("Direction"), "ltr, rtl");
    assert!(matches!(
        workshop.state().matrix_axes().peek(),
        MatrixAxes::Picked(_)
    ));
}

#[test]
fn the_first_cell_is_focused_and_a_press_focuses_another_for_the_panels_to_follow() {
    let mut workshop = Workshop::open(BOTH_WAYS);
    let state = workshop.state().clone();
    state.panel_tab().set(PanelTab::Layout);
    workshop.settle();
    assert_eq!(
        state.matrix_focus().peek().as_deref(),
        Some("kit--badge--both-ways--dir=ltr")
    );
    assert_eq!(
        workshop.node(Role::Button, "Focus dir: ltr").toggled,
        Some(true)
    );
    assert!(!state.inspection().nodes().is_empty());
    let first = state.inspection().placement();

    workshop.click(Role::Button, "Focus dir: rtl");
    assert_eq!(
        state.matrix_focus().peek().as_deref(),
        Some("kit--badge--both-ways--dir=rtl")
    );
    assert_eq!(
        workshop.node(Role::Button, "Focus dir: rtl").toggled,
        Some(true)
    );
    assert_eq!(
        workshop.node(Role::Button, "Focus dir: ltr").toggled,
        Some(false)
    );
    state.panel_tab().set(PanelTab::Accessibility);
    workshop.settle();
    state.panel_tab().set(PanelTab::Layout);
    workshop.settle();
    let second = state.inspection().placement();
    assert!(
        second.e > first.e,
        "the panels follow the rtl cell, right of the ltr one: {first:?} then {second:?}"
    );
}

#[test]
fn open_in_canvas_shows_the_focused_cell_on_the_canvas_view() {
    let mut workshop = Workshop::open(PLAIN);
    let state = workshop.state().clone();
    state.matrix_axes().set(MatrixAxes::Picked(vec![
        PickedAxis::all_of(CustomAxis::Direction),
        PickedAxis::all_of(CustomAxis::Arg("ghost".into())),
    ]));
    workshop.settle();
    workshop.click(Role::Button, "Focus dir: rtl · ghost: true");
    workshop.click(Role::Button, "Open in canvas");
    assert_eq!(state.view().peek(), ViewMode::Canvas);
    assert_eq!(
        state.canvas_settings().peek().direction,
        Some(Direction::Rtl)
    );
    let plain = state.entries()[2];
    let ghost = state
        .args(&plain)
        .states()
        .into_iter()
        .find(|arg| arg.name == "ghost")
        .and_then(|arg| arg.value);
    assert_eq!(ghost, Some(ArgValue::Bool(true)));
}

#[test]
fn a_named_overlay_opened_in_one_cell_stays_in_that_cell() {
    let mut workshop = Workshop::open(ASKING);
    let asks: Vec<AccessNode> = workshop
        .nodes()
        .into_iter()
        .filter(|node| node.role == Role::Button && node.name == "Ask")
        .collect();
    assert_eq!(asks.len(), 2, "one per cell");
    let (x, y) = centre(asks[0].rect);
    workshop.send(press(x, y));
    workshop.send(release(x, y));
    assert_eq!(
        workshop.count("Sure?"),
        1,
        "the other cell's confirm stays closed: {:?}",
        workshop.texts()
    );
}

#[test]
fn the_view_stops_at_the_cap_and_says_how_many_cells_it_left_out() {
    const AXES: &[Axis] = &[Axis::Arg("a", NINE), Axis::Arg("b", NINE)];
    let entry = PreviewEntry::new("kit--many--cells", "many", "Cells", widget);
    let every = Matrix::expand(AXES, &entry, &Matrices::default()).unwrap();
    assert_eq!(every.len(), 81, "expanding never caps");
    let capped = plan::plan(
        &entry.matrix(Matrix::Axes(AXES)),
        &MatrixAxes::Declared,
        &[],
        &Matrices::default(),
    )
    .unwrap();
    assert_eq!(capped.cells.len(), telar::preview::MATRIX_CELL_CAP);
    assert_eq!(capped.total, 81);
    assert!(capped.capped());
    assert_eq!(capped.cells, every[..64]);
}

#[test]
fn a_capped_matrix_mounts_the_first_cells_and_warns() {
    reset_layout_runtime();
    focus::clear();
    let entries = vec![
        PreviewEntry::new("kit--many--cells", "many", "Cells", widget)
            .matrix(Matrix::Axes(&[Axis::Arg("a", NINE), Axis::Arg("b", NINE)])),
        PreviewEntry::new("kit--some--cells", "some", "Cells", widget).matrix(Matrix::Axes(&[
            Axis::Arg("a", SEVEN),
            Axis::Arg("b", SEVEN),
        ])),
    ];
    let state = WorkshopState::new(entries.into(), &Default::default());
    state.view().set(ViewMode::Matrix);
    let runtime = LocalApp(Shell { state });
    let tree = mount(|| runtime.0.root(), WIDTH, HEIGHT);
    settle(&tree);
    let cells = runtime
        .access_snapshot(&tree.commands())
        .into_iter()
        .filter(|node| node.role == Role::Button && node.name.starts_with("Focus "))
        .count();
    assert_eq!(cells, 64);
    assert!(
        texts(&tree)
            .iter()
            .any(|text| text
                == "Showing the first 64 of 81 cells: narrow the matrix to see the rest"),
        "{:?}",
        texts(&tree)
    );
    assert!(
        texts(&tree).iter().filter(|text| *text == "widget").count() < 64,
        "cells out of view are not mounted"
    );

    runtime.0.state.select("kit--some--cells");
    settle(&tree);
    assert!(
        texts(&tree)
            .iter()
            .any(|text| text == "49 cells take a while to mount"),
        "{:?}",
        texts(&tree)
    );
}

#[test]
fn a_grid_puts_the_last_axis_across_and_the_others_down() {
    let entry = PreviewEntry::new("kit--x--y", "x", "Y", widget);
    let cells = Matrix::expand(
        &[
            Axis::Mode(&["light", "dark"]),
            Axis::Locale(&["en"]),
            Axis::Dir(&[Direction::Ltr, Direction::Rtl]),
        ],
        &entry,
        &Matrices::default(),
    )
    .unwrap();
    let laid = grid(&cells);
    assert_eq!(laid.columns, ["dir: ltr", "dir: rtl"]);
    let headings: Vec<_> = laid
        .rows
        .iter()
        .map(|row| (row.heading.clone().unwrap(), row.cells.len()))
        .collect();
    assert_eq!(
        headings,
        [
            ("mode: light · locale: en".to_string(), 2),
            ("mode: dark · locale: en".to_string(), 2),
        ]
    );

    let one = Matrix::expand(&[Axis::Locale(&["en", "ar"])], &entry, &Matrices::default()).unwrap();
    let laid = grid(&one);
    assert_eq!(laid.columns, ["locale: en", "locale: ar"]);
    assert_eq!(laid.rows.len(), 1);
    assert_eq!(laid.rows[0].heading, None, "one axis needs no row heading");

    let cut = grid(&cells[..3]);
    assert_eq!(cut.rows[1].cells.len(), 1, "a row the cap cut short");
}

#[test]
fn the_axes_picked_nest_the_same_way_whatever_order_they_were_picked_in() {
    let names = |picked: &[PickedAxis]| {
        picked
            .iter()
            .map(|picked| picked.axis.clone())
            .collect::<Vec<_>>()
    };
    let axes = toggled(&[], &CustomAxis::Arg("size".into()), true);
    let axes = toggled(&axes, &CustomAxis::Direction, true);
    let axes = toggled(&axes, &CustomAxis::Mode, true);
    assert_eq!(
        names(&axes),
        [
            CustomAxis::Mode,
            CustomAxis::Direction,
            CustomAxis::Arg("size".into())
        ]
    );
    assert_eq!(
        names(&toggled(&axes, &CustomAxis::Direction, false)),
        [CustomAxis::Mode, CustomAxis::Arg("size".into())]
    );
}

#[test]
fn a_built_matrix_varies_the_globals_and_the_args_with_a_fixed_set_of_values() {
    const WATCH: &[ViewportPreset] = &[ViewportPreset::new("watch", 198.0, 242.0)];
    let project = Matrices::new(&[], WATCH);
    let viewports = CustomAxis::Viewport.axis(&[], &project);
    assert_eq!(viewports, Some(Axis::ProjectViewports));
    assert_eq!(
        CustomAxis::Viewport.axis(&[], &Matrices::default()),
        Some(Axis::Viewport(crate::settings::PRESET_VIEWPORTS))
    );
    assert_eq!(
        CustomAxis::ControlSize
            .axis(&[], &Matrices::default())
            .map(|axis| Matrix::count(&[axis], &Matrices::default())),
        Some(4)
    );
    assert_eq!(CustomAxis::Arg("missing".into()).axis(&[], &project), None);

    let entry = PreviewEntry::new(PLAIN, "badge", "Plain", widget).args(&GHOST);
    let args = telar::preview::host::Args::in_memory_for(&entry).states();
    assert_eq!(plan::varied_args(&args), ["ghost"]);
    assert_eq!(
        CustomAxis::Arg("ghost".into()).axis(&args, &project),
        Some(Axis::Arg("ghost", &["false", "true"]))
    );
}

#[test]
fn what_keeps_a_plan_from_having_cells_is_told_apart() {
    let plain = PreviewEntry::new(PLAIN, "badge", "Plain", widget);
    let none = Matrices::default();
    let plan_of = |entry: &PreviewEntry, axes: MatrixAxes| plan::plan(entry, &axes, &[], &none);
    let picked = |axes: &[CustomAxis]| {
        MatrixAxes::Picked(axes.iter().cloned().map(PickedAxis::all_of).collect())
    };
    assert_eq!(
        plan_of(&plain, MatrixAxes::Declared),
        Err(Problem::NoMatrix)
    );
    let themed = plain.matrix(Matrix::Named("themes"));
    assert_eq!(
        plan_of(&themed, MatrixAxes::Declared),
        Err(Problem::Empty),
        "no mode is registered"
    );
    assert_eq!(plan_of(&themed, picked(&[])), Err(Problem::NoAxes));
    let unknown = plain.matrix(Matrix::Named("nowhere"));
    assert!(matches!(
        plan_of(&unknown, MatrixAxes::Declared),
        Err(Problem::Invalid(_))
    ));
    assert_eq!(
        plan_of(&plain, picked(&[CustomAxis::Direction])).map(|plan| plan.total),
        Ok(2)
    );
    let narrowed = plain.matrix(Matrix::Axes(&[Axis::Dir(&[Direction::Rtl])]));
    let direction = |declared| {
        MatrixAxes::Picked(vec![PickedAxis {
            axis: CustomAxis::Direction,
            declared,
        }])
    };
    assert_eq!(
        plan_of(&narrowed, direction(true)).map(|plan| plan.total),
        Ok(1),
        "the preview's own matrix narrows the axis"
    );
    assert_eq!(
        plan_of(&narrowed, direction(false)).map(|plan| plan.total),
        Ok(2)
    );
}

#[test]
fn a_cell_is_set_up_in_the_toolbar_settings_under_its_own() {
    let entry = PreviewEntry::new(PLAIN, "badge", "Plain", widget)
        .env(PreviewEnv::NONE.locale("en").mode("day"));
    let cell = Matrix::expand(&[Axis::Locale(&["ar"])], &entry, &Matrices::default())
        .unwrap()
        .remove(0);
    let settings = CanvasSettings {
        mode: ModeChoice::Named("night".into()),
        locale: Some("fr".into()),
        control_size: Some(ControlSize::Small),
        background: Background::Dark,
        ..CanvasSettings::default()
    };
    let globals = Globals::seeded(&entry.env);
    seed(&globals, &settings, ColorScheme::Light, &entry, &cell);
    assert_eq!(globals.mode().peek().as_deref(), Some("night"));
    assert_eq!(
        globals.locale().peek().as_deref(),
        Some("ar"),
        "the cell wins"
    );
    assert_eq!(
        globals.direction().peek(),
        Some(Direction::Rtl),
        "read in the cell's locale's direction"
    );
    assert_eq!(globals.control_size().peek(), Some(ControlSize::Small));
    assert_eq!(
        globals.background().peek(),
        Some(crate::settings::DARK_BACKGROUND)
    );
    assert_eq!(globals.viewport().peek(), None);
}

#[test]
fn a_snapshot_that_failed_the_run_reads_as_drifted() {
    for status in [SnapshotStatus::Matched, SnapshotStatus::Updated] {
        assert_eq!(Status::of_snapshot(status), Status::SnapshotMatches);
    }
    for status in [
        SnapshotStatus::Changed,
        SnapshotStatus::Missing,
        SnapshotStatus::Failed,
    ] {
        assert_eq!(Status::of_snapshot(status), Status::SnapshotDrifted);
    }
}

#[test]
fn a_cell_without_a_viewport_mounts_at_the_size_the_test_run_uses_and_is_only_shrunk_to_show() {
    let entry = PreviewEntry::new(PLAIN, "badge", "Plain", widget);
    let cell = Matrix::expand(&[Axis::Locale(&["ar"])], &entry, &Matrices::default())
        .unwrap()
        .remove(0);

    let (viewport, scale) = sizing(&entry, &cell);
    assert_eq!(viewport, Play::DEFAULT_VIEWPORT);
    assert!(scale < 1.0, "an 800 wide canvas is shrunk to fit its cell");

    let named = entry.env(PreviewEnv::NONE.viewport(200.0, 100.0));
    let (viewport, scale) = sizing(&named, &cell);
    assert_eq!(viewport, Size::new(200.0, 100.0));
    assert_eq!(scale, 1.0, "a canvas smaller than its cell is not enlarged");
}

#[test]
fn a_play_that_clicks_sees_the_reactive_result_in_its_cell() {
    let workshop = Workshop::open(COUNTING);
    assert!(workshop.draws("Play passed"), "{:?}", workshop.texts());
    assert!(workshop.draws("Pressed 1"), "{:?}", workshop.texts());
}

/// A cell of the themes matrix opens on the canvas in its mode, which is told for a scheme from the theme's own signals: read while the settings were held, it panicked.
#[test]
fn open_in_canvas_takes_the_focused_cell_s_mode() {
    register_modes();
    let mut workshop = Workshop::open(THEMED);
    let state = workshop.state().clone();
    workshop.click(Role::Button, "Focus mode: night");
    workshop.click(Role::Button, "Open in canvas");
    assert_eq!(state.view().peek(), ViewMode::Canvas);
    assert_eq!(
        state.canvas_settings().peek().mode,
        crate::settings::ModeChoice::for_mode("night")
    );
}

/// A play is written against the preview's args, so a matrix built from the axes picked runs it only where they are the defaults, while the preview's own matrix, which its author declared the play holds across, runs it everywhere.
#[test]
fn a_picked_arg_axis_runs_the_play_only_where_the_args_are_the_defaults() {
    let mut workshop = Workshop::open(BOTH_WAYS);
    workshop.pick("ghost");
    workshop.next_turn();
    assert_eq!(workshop.cells().len(), 4);
    assert_eq!(workshop.count("Play passed"), 2, "{:?}", workshop.texts());
    assert_eq!(workshop.count("Play skipped"), 2);

    workshop.reset_axes();
    workshop.next_turn();
    assert_eq!(workshop.count("Play passed"), 2);
    assert_eq!(workshop.count("Play skipped"), 0);
}

/// Why a cell failed takes one line however long it runs, so a failed cell stands as tall as one that passed.
#[test]
fn a_cell_s_failure_takes_one_line() {
    let workshop = Workshop::open(LONG_FAILING);
    let line = workshop
        .texts()
        .into_iter()
        .find(|text| text.contains("runs well past one cell"))
        .unwrap_or_else(|| panic!("the failure is shown: {:?}", workshop.texts()));
    let rect = telar::testing::rect_of(&workshop.tree, &line).expect("drawn");
    assert!(rect.height < 24.0, "one line, not {rect:?}");
}

/// An arg the preview reads as it builds, rather than declares, is an axis to pick before the canvas has ever shown the preview: the matrix builds it once off screen to learn it.
#[test]
fn an_arg_read_at_build_is_an_axis_without_visiting_the_canvas() {
    let mut workshop = Workshop::open(LIT);
    workshop.pick("lit");
    assert_eq!(workshop.cells(), ["Focus lit: false", "Focus lit: true"]);
}

#[test]
fn an_axis_s_tooltip_lists_the_values_it_takes() {
    let entry = PreviewEntry::new(PLAIN, "badge", "Plain", widget);
    let values = super::axis_values(
        Axis::Dir(&[Direction::Ltr, Direction::Rtl]),
        &entry,
        &Matrices::default(),
    );
    assert_eq!(values, "ltr, rtl");
}

/// The header stands as tall as the canvas's whatever the matrix can vary, its axes behind a menu rather than wrapped onto more lines.
#[test]
fn the_header_is_as_tall_as_the_canvas_header_and_keeps_its_axes_in_a_menu() {
    let workshop = Workshop::open(BOTH_WAYS);
    let menu = workshop.node(Role::Button, AXES_MENU).rect;
    let open = workshop.node(Role::Button, "Open in canvas").rect;
    let header = crate::canvas::HEADER_HEIGHT;
    let top = workshop.node(Role::Button, "Focus dir: ltr").rect.y;
    assert!(
        menu.y + menu.height <= top - header / 2.0,
        "{menu:?} over {top}"
    );
    assert!(
        (menu.y - open.y).abs() < 1.0,
        "on one line: {menu:?} {open:?}"
    );
    assert!(
        workshop
            .nodes()
            .iter()
            .all(|node| node.role != Role::CheckBox),
        "no axis is picked in the header itself"
    );
}

#[test]
fn a_secondary_press_on_a_cell_opens_it_in_the_canvas() {
    let mut workshop = Workshop::open(BOTH_WAYS);
    workshop.secondary_click(Role::Button, "Focus dir: rtl");
    assert_eq!(
        workshop.state().matrix_focus().peek().as_deref(),
        Some("kit--badge--both-ways--dir=rtl")
    );
    workshop.click(Role::MenuItem, "Open in canvas");
    let state = workshop.state();
    assert_eq!(state.view().peek(), ViewMode::Canvas);
    assert_eq!(
        state.canvas_settings().peek().direction,
        Some(Direction::Rtl)
    );
}

/// Turning an axis leaves the menu open, and the row that resets the axes comes and goes in it as they stop and start being the preview's own.
#[test]
fn the_reset_row_comes_and_goes_while_the_menu_is_open() {
    let mut workshop = Workshop::open(BOTH_WAYS);
    workshop.click(Role::Button, AXES_MENU);
    let reset = |workshop: &Workshop| {
        workshop
            .nodes()
            .iter()
            .any(|node| node.role == Role::MenuItem && node.name == "Reset the axes")
    };
    assert!(!reset(&workshop));
    let (x, y) = centre(workshop.axis_row("ghost").rect);
    workshop.send(press(x, y));
    workshop.send(release(x, y));
    assert!(reset(&workshop), "there without opening the menu again");
    workshop.click(Role::MenuItem, "Reset the axes");
    assert_eq!(workshop.state().matrix_axes().peek(), MatrixAxes::Declared);
}
