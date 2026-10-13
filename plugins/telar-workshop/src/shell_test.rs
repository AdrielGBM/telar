use std::rc::Rc;
use std::time::Duration;

use telar::focus;
use telar::inspect::FrameAccess;
use telar::inspect::a11y::Severity;
use telar::preview::{PreviewCtx, PreviewEntry, by_role};
use telar::testing::{centre, key_with, mount, named, press, release, route, texts};
use telar::{
    AccessNode, App, AppRuntime, Children, ComponentList, DrawCommand, Event, Key, LayoutError,
    LayoutItem, LocalApp, ModifiersState, NamedKey, NodeId, Reactive, Rect, Role, Size,
    reset_layout_runtime, signal,
};
use telar_components::{ButtonProps, button};

use crate::WorkshopApp;
use crate::arrangement::PANES_FROM;
use crate::settings::CanvasSettings;
use crate::state::{PANEL_DEFAULT_SIZE, PanelPosition, WorkshopState};
use crate::test_support::{Shell, control, sample_previews, setting_row, settle};
use telar::Direction;

const HEIGHT: u32 = 800;
const WIDE: u32 = 1400;
const MEDIUM: u32 = 1000;
const NARROW: u32 = 800;
const PROJECT: &str = "bench";

/// The workshop as an application runs it, over [`sample_previews`].
struct Workshop<A: App> {
    runtime: LocalApp<A>,
    tree: ComponentList,
    width: u32,
}

impl Workshop<WorkshopApp> {
    fn app(width: u32, project: &str) -> Self {
        Self::mount(
            WorkshopApp::new(sample_previews()).project_name(project),
            width,
        )
    }
}

impl Workshop<Shell> {
    fn shell(width: u32) -> Self {
        Self::shell_over(sample_previews(), width)
    }

    fn shell_over(previews: Vec<PreviewEntry>, width: u32) -> Self {
        reset_layout_runtime();
        focus::clear();
        let state = WorkshopState::new(previews.into(), &Default::default());
        Self::mount(Shell { state }, width)
    }

    /// Lets the turn of the loop a canvas waits for pass, so a play due to run does.
    fn next_turn(&mut self) {
        telar::testing::advance_time(Duration::ZERO);
        settle(&self.tree);
    }

    fn draws(&self, text: &str) -> bool {
        texts(&self.tree).iter().any(|drawn| drawn == text)
    }

    fn state(&self) -> &WorkshopState {
        &self.runtime.0.state
    }

    fn settings(&self) -> CanvasSettings {
        self.state().canvas_settings().get()
    }
}

impl<A: App> Workshop<A> {
    fn mount(app: A, width: u32) -> Self {
        telar::install_default_text_metrics();
        telar::set_surface_size(Size::new(width as f32, HEIGHT as f32));
        let runtime = LocalApp(app);
        let tree = mount(runtime.0.root(), width, HEIGHT);
        settle(&tree);
        Self {
            runtime,
            tree,
            width,
        }
    }

    fn nodes(&self) -> Vec<AccessNode> {
        self.runtime.access_snapshot(&self.tree.commands())
    }

    fn find(&self, role: Role, name: &str) -> Option<AccessNode> {
        self.nodes()
            .into_iter()
            .find(|node| node.role == role && node.name == name)
    }

    /// Every landmark drawn, in document order, each as the path of landmarks down to it.
    fn landmarks(&self) -> Vec<String> {
        let mut found = Vec::new();
        self.walk_elements(|role, name, around| {
            if role.is_region() {
                let mut path = around.to_vec();
                path.push(landmark_name(role, name));
                found.push(path.join(" > "));
            }
            false
        });
        found
    }

    /// The landmarks the box of `role` announced as `name` sits in, outermost first.
    fn landmarks_around(&self, role: Role, name: &str) -> Vec<String> {
        let mut found = None;
        self.walk_elements(|drawn, named, around| {
            let hit = drawn == role && named == Some(name);
            if hit {
                found = Some(around.to_vec());
            }
            hit
        });
        found.unwrap_or_else(|| panic!("no {role:?} is drawn as {name}"))
    }

    /// Calls `visit` with each box drawn, its name and the landmarks around it, until it answers `true`.
    fn walk_elements(&self, mut visit: impl FnMut(Role, Option<&str>, &[String]) -> bool) {
        let commands = self.tree.commands();
        let reading = FrameAccess::read(&commands).reading;
        let mut open: Vec<Option<String>> = Vec::new();
        for command in commands.iter() {
            match command {
                DrawCommand::PushElement { element } => {
                    let role = element.semantics.role;
                    let name = reading
                        .get(NodeId::from(element.id.0))
                        .and_then(|box_reading| box_reading.name.as_deref());
                    let around: Vec<String> = open.iter().flatten().cloned().collect();
                    if visit(role, name, &around) {
                        return;
                    }
                    open.push(role.is_region().then(|| landmark_name(role, name)));
                }
                DrawCommand::PopElement => {
                    open.pop();
                }
                _ => {}
            }
        }
    }

    fn region(&self, role: Role, name: &str) -> Rect {
        self.find(role, name)
            .unwrap_or_else(|| panic!("no {role:?} is announced as {name}"))
            .rect
    }

    fn control(&self, role: Role, name: &str) -> AccessNode {
        control(&self.nodes(), role, name)
    }

    fn send(&mut self, event: Event) {
        route(&mut self.tree, &event);
        settle(&self.tree);
    }

    fn click(&mut self, role: Role, name: &str) {
        let (x, y) = centre(self.control(role, name).rect);
        self.send(press(x, y));
        self.send(release(x, y));
    }

    /// Opens the submenu of the preview setting `name`, in the settings menu already open.
    fn open_setting(&mut self, name: &str) {
        let (x, y) = centre(setting_row(&self.nodes(), name).rect);
        self.send(press(x, y));
        self.send(release(x, y));
    }

    fn alt(&mut self, key: Key) {
        let modifiers = ModifiersState {
            is_alt: true,
            ..ModifiersState::default()
        };
        self.send(key_with(key, modifiers));
    }

    fn resize(&mut self, width: u32) {
        self.width = width;
        telar::set_surface_size(Size::new(width as f32, HEIGHT as f32));
        self.send(Event::WindowResized {
            width,
            height: HEIGHT,
        });
    }

    /// What checking the window finds that keeps someone from using it, and every control drawn even partly outside the window.
    fn problems(&self) -> Vec<String> {
        let background = self.runtime.0.clear_color();
        let audit = telar::inspect::audit(&self.tree.commands(), background);
        let window = Rect::new(0.0, 0.0, self.width as f32, HEIGHT as f32);
        let errors = audit
            .violations
            .iter()
            .filter(|violation| violation.severity == Severity::Error)
            .map(|violation| format!("{violation} at {:?}", violation.rect));
        let outside = audit
            .nodes
            .iter()
            .filter(|node| {
                node.id.is_some() && node.role.is_control() && !within(node.rect, window)
            })
            .map(|node| {
                format!(
                    "{:?} {:?} at {:?} is outside the window",
                    node.role, node.name, node.rect
                )
            });
        errors.chain(outside).collect()
    }

    fn assert_passes(&self, shown: &str) {
        let problems = self.problems();
        assert!(
            problems.is_empty(),
            "{shown} {} wide:\n{}",
            self.width,
            problems.join("\n")
        );
    }
}

fn landmark_name(role: Role, name: Option<&str>) -> String {
    match name {
        Some(name) => format!("{} {name}", role.as_str()),
        None => role.as_str().to_string(),
    }
}

fn within(inner: Rect, outer: Rect) -> bool {
    const SLACK: f32 = 0.5;
    inner.x >= outer.x - SLACK
        && inner.y >= outer.y - SLACK
        && inner.x + inner.width <= outer.x + outer.width + SLACK
        && inner.y + inner.height <= outer.y + outer.height + SLACK
}

/// Sprint 4's acceptance for the workshop: the chrome passes the checks it runs on a preview, at every width it arranges itself for and with everything it folds away opened.
#[test]
fn the_chrome_passes_its_own_accessibility_checks_in_a_wide_window() {
    let mut workshop = Workshop::app(WIDE, PROJECT);
    workshop.assert_passes("the workshop");
    workshop.click(Role::Button, "Settings");
    workshop.open_setting("Direction");
    assert!(
        workshop
            .find(Role::MenuItemRadio, "Right to left")
            .is_some()
    );
    workshop.assert_passes("the preview settings menu, a submenu open");
}

#[test]
fn the_chrome_passes_its_own_accessibility_checks_in_a_medium_window() {
    let mut workshop = Workshop::app(MEDIUM, PROJECT);
    workshop.assert_passes("the workshop");
    workshop.click(Role::Button, "Settings");
    workshop.open_setting("Direction");
    assert!(
        workshop
            .find(Role::MenuItemRadio, "Right to left")
            .is_some()
    );
    workshop.assert_passes("the preview settings menu, a submenu open");
}

#[test]
fn the_chrome_passes_its_own_accessibility_checks_in_a_narrow_window() {
    let mut workshop = Workshop::app(NARROW, PROJECT);
    workshop.assert_passes("the workshop");
    workshop.click(Role::Button, "Previews");
    assert!(workshop.find(Role::Dialog, "Previews").is_some());
    workshop.assert_passes("the sidebar's drawer");
    workshop.send(named(NamedKey::Escape));
    workshop.send(named(NamedKey::Escape));
    assert!(
        workshop.find(Role::Dialog, "Previews").is_none(),
        "Escape takes the keyboard from the search field to the tree, then shuts the drawer"
    );
    workshop.click(Role::Button, "Settings");
    workshop.assert_passes("the preview settings menu");
}

#[test]
fn a_long_project_name_is_cut_short_rather_than_pushing_the_top_bar_out_of_the_window() {
    let workshop = Workshop::app(NARROW, "a-design-system-with-a-very-long-package-name");
    workshop.assert_passes("the workshop");
}

#[test]
fn the_preview_settings_menu_sets_the_environment_of_every_canvas() {
    let mut workshop = Workshop::shell(MEDIUM);
    workshop.click(Role::Button, "Settings");
    workshop.open_setting("Direction");
    workshop.click(Role::MenuItemRadio, "Right to left");
    assert_eq!(workshop.settings().direction, Some(Direction::Rtl));
    assert!(
        workshop
            .find(Role::MenuItemRadio, "Right to left")
            .is_none(),
        "picking a row shuts the menu"
    );

    workshop.click(Role::Button, "Settings");
    workshop.click(Role::MenuItemCheckBox, "High contrast");
    assert_eq!(workshop.settings().high_contrast, Some(true));
    assert!(
        workshop
            .find(Role::MenuItemCheckBox, "High contrast")
            .is_some(),
        "a switch row leaves the menu open"
    );
}

const WORKSPACE: &str = "main Workspace";

/// What axe's `region` rule asks of the page: the canvas and the panels are regions of the workspace, the one main landmark, and both splitters lie inside it rather than between landmarks.
#[test]
fn the_workspace_is_the_main_landmark_and_holds_both_splitters() {
    let workshop = Workshop::shell(WIDE);
    assert_eq!(
        workshop.landmarks(),
        [
            "banner",
            "navigation Previews",
            WORKSPACE,
            "main Workspace > section Canvas",
            "main Workspace > section Panels",
        ]
    );
    for splitter in ["Resize the sidebar", "Resize the panels"] {
        assert_eq!(
            workshop.landmarks_around(Role::Splitter, splitter),
            [WORKSPACE],
            "{splitter}"
        );
    }
}

#[test]
fn the_sidebar_splitter_stays_in_the_workspace_while_the_sidebar_is_folded() {
    let workshop = Workshop::shell(WIDE);
    workshop.state().sidebar_collapsed().set(true);
    workshop.state().panel_position().set(PanelPosition::Right);
    settle(&workshop.tree);
    assert!(workshop.find(Role::Navigation, "Previews").is_none());
    for splitter in ["Resize the sidebar", "Resize the panels"] {
        assert_eq!(
            workshop.landmarks_around(Role::Splitter, splitter),
            [WORKSPACE],
            "{splitter}"
        );
    }
}

#[test]
fn a_narrow_window_keeps_the_workspace_the_main_landmark() {
    let workshop = Workshop::shell(NARROW);
    assert_eq!(
        workshop.landmarks(),
        [
            "banner",
            WORKSPACE,
            "main Workspace > section Canvas",
            "main Workspace > section Panels",
        ]
    );
    assert_eq!(
        workshop.landmarks_around(Role::Splitter, "Resize the panels"),
        [WORKSPACE]
    );
}

#[test]
fn a_narrow_window_shows_the_sidebar_in_a_drawer_that_a_pick_shuts() {
    let mut workshop = Workshop::shell(NARROW);
    assert!(workshop.find(Role::Navigation, "Previews").is_none());
    assert!(
        workshop
            .find(Role::Splitter, "Resize the sidebar")
            .is_none()
    );
    assert_eq!(
        workshop.region(Role::Section, "Canvas").width,
        NARROW as f32,
        "the canvas takes the window's width"
    );

    workshop.click(Role::Button, "Previews");
    assert!(workshop.state().sidebar_drawer().get());
    workshop.click(Role::TreeItem, "Ghost");
    assert_eq!(
        workshop.state().selection().get().as_deref(),
        Some("sample--button--ghost")
    );
    assert!(!workshop.state().sidebar_drawer().get());
    assert!(workshop.find(Role::Dialog, "Previews").is_none());
}

#[test]
fn the_sidebar_shortcut_opens_the_drawer_in_a_narrow_window_and_folds_the_pane_in_a_wider_one() {
    let mut workshop = Workshop::shell(NARROW);
    workshop.alt(Key::Char('s'));
    assert!(workshop.find(Role::Dialog, "Previews").is_some());
    assert!(!workshop.state().sidebar_collapsed().get());

    workshop.resize(MEDIUM);
    assert!(
        !workshop.state().sidebar_drawer().get(),
        "a window wide enough for the pane shuts the drawer"
    );
    assert!(workshop.find(Role::Navigation, "Previews").is_some());
    assert!(
        focus::text_entry_focused(),
        "the search field the drawer opened on moved into the pane with the keyboard still in it"
    );
    workshop.send(named(NamedKey::Escape));
    workshop.alt(Key::Char('s'));
    assert!(workshop.state().sidebar_collapsed().get());

    workshop.resize(NARROW);
    assert!(
        workshop.find(Role::Dialog, "Previews").is_none(),
        "narrowing the window again leaves the drawer shut"
    );
}

#[test]
fn a_narrow_window_puts_the_panels_below_the_canvas_wherever_they_were_put() {
    let mut workshop = Workshop::shell(MEDIUM);
    workshop.state().panel_position().set(PanelPosition::Right);
    settle(&workshop.tree);
    let canvas = workshop.region(Role::Section, "Canvas");
    let panels = workshop.region(Role::Section, "Panels");
    assert!(panels.x >= canvas.x + canvas.width, "beside the canvas");

    workshop.resize(NARROW);
    let canvas = workshop.region(Role::Section, "Canvas");
    let panels = workshop.region(Role::Section, "Panels");
    assert!(panels.y >= canvas.y + canvas.height, "below the canvas");
    assert_eq!(panels.width, NARROW as f32);
    assert_eq!(panels.height, PANEL_DEFAULT_SIZE);
    assert_eq!(
        workshop.state().panel_position().get(),
        PanelPosition::Right,
        "where they were put is kept for a wider window"
    );
}

#[test]
fn the_panels_splitter_moves_with_the_arrow_keys_in_a_narrow_window() {
    let mut workshop = Workshop::shell(NARROW);
    workshop.click(Role::Splitter, "Resize the panels");
    workshop.send(named(NamedKey::ArrowUp));
    let grown = workshop.state().panel_size().get();
    assert!(grown > PANEL_DEFAULT_SIZE, "{grown}");
    assert_eq!(workshop.region(Role::Section, "Panels").height, grown);
}

#[test]
fn the_window_width_picks_the_arrangement() {
    use crate::arrangement::Arrangement;
    let mut workshop = Workshop::shell(PANES_FROM as u32 - 1);
    let arrangement = workshop.state().arrangement();
    assert_eq!(arrangement.get(), Arrangement::Compact);
    workshop.resize(PANES_FROM as u32);
    assert_eq!(arrangement.get(), Arrangement::Wide);
    telar::set_surface_size(Size::ZERO);
    assert_eq!(
        arrangement.get(),
        Arrangement::Wide,
        "a surface with no size yet folds nothing away"
    );
}

fn counter(_: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let presses = signal(0_u32);
    button(
        ButtonProps::props()
            .label(Reactive::of(move || format!("Pressed {}", presses.get())))
            .on_press(Rc::new(move || presses.update(|count| *count += 1)))
            .build(),
        Children::default(),
    )
}

fn played() -> Vec<PreviewEntry> {
    vec![
        PreviewEntry::new("plays--counter--passing", "counter", "Passing", counter).play(
            |canvas| {
                canvas.click(by_role(Role::Button).named("Pressed 0"))?;
                canvas.expect_text("Pressed 1")
            },
        ),
    ]
}

/// The canvas is built once and moved between the arrangements, so the preview mounted on it, the play that ran there and the step picked from that run all outlive a window crossing a breakpoint, and the panels moving to another edge.
#[test]
fn rearranging_the_workshop_moves_the_canvas_instead_of_mounting_it_again() {
    let mut workshop = Workshop::shell_over(played(), WIDE);
    workshop.next_turn();
    let interactions = workshop.state().interactions().clone();
    let run = interactions
        .run()
        .expect("the play ran as the canvas mounted");
    assert_eq!(run.mount, 1);
    assert!(
        workshop.draws("Pressed 1"),
        "the play pressed the preview once"
    );
    interactions.pick(Some(0));
    assert_eq!(interactions.picked(), Some(0));

    workshop.resize(NARROW);
    workshop.next_turn();
    assert!(workshop.state().arrangement().get().sidebar_in_drawer());
    let run = interactions.run().expect("the run is kept");
    assert_eq!(run.mount, 1, "crossing the breakpoint mounted nothing");
    assert!(
        run.ran,
        "the run is still the one that ran, not one waiting to run again"
    );
    assert_eq!(
        interactions.picked(),
        Some(0),
        "the step picked from it is still picked"
    );

    workshop.resize(WIDE);
    workshop.state().panel_position().set(PanelPosition::Right);
    workshop.next_turn();
    assert_eq!(interactions.run().map(|run| run.mount), Some(1));
    assert_eq!(interactions.picked(), Some(0));

    interactions.back_to_live();
    settle(&workshop.tree);
    assert!(
        workshop.draws("Pressed 1"),
        "the preview is the one the play pressed, not a fresh one"
    );
    let canvas = workshop.region(Role::Section, "Canvas");
    let panels = workshop.region(Role::Section, "Panels");
    assert!(
        panels.x >= canvas.x + canvas.width,
        "the panels moved beside the canvas"
    );
}
