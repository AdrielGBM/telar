use std::time::Duration;

use telar::focus;
use telar::testing::{Scoped, centre, mount, moved, press, release, route, texts};
use telar::{
    AccessNode, App, AppRuntime, Component, ComponentList, Event, LocalApp, Rect, ResizeEdge, Role,
    Size, WindowCommand, WindowRoot, reset_layout_runtime, take_window_commands,
};
use telar_components::DOUBLE_CLICK;
use telar_devtools::workbench_scope;

use super::*;
use crate::arrangement::FIND_FIELD_COMFORT;
use crate::test_support::{sample_previews, settle};

const HEIGHT: u32 = 800;
const WIDE: u32 = 1400;
const MEDIUM: u32 = 1000;
const PROJECT: &str = "catalogue";

struct Framed {
    state: WorkshopState,
    project: Option<String>,
    frame: Frame,
}

impl App for Framed {
    fn root(&self) -> Box<dyn Component> {
        let chrome = workbench_scope(|| {
            crate::shell::shell_in(&self.state, self.project.clone(), self.frame)
        })
        .expect("the chrome builds");
        Box::new(WindowRoot::wrapping(Box::new(chrome)).expect("the window builds"))
    }
}

struct Workshop {
    runtime: LocalApp<Framed>,
    tree: Scoped<ComponentList>,
}

impl Workshop {
    fn open(frame: Frame, width: u32) -> Self {
        Self::named(frame, width, Some(PROJECT))
    }

    fn named(frame: Frame, width: u32, project: Option<&str>) -> Self {
        reset_layout_runtime();
        focus::clear();
        telar::install_default_text_metrics();
        telar::set_surface_size(Size::new(width as f32, HEIGHT as f32));
        telar::set_surface_maximized(false);
        take_window_commands();
        let state = WorkshopState::new(sample_previews().into(), &Default::default());
        let runtime = LocalApp(Framed {
            state,
            project: project.map(str::to_owned),
            frame,
        });
        let tree = mount(|| runtime.0.root(), width, HEIGHT);
        settle(&tree);
        Self { runtime, tree }
    }

    fn nodes(&self) -> Vec<AccessNode> {
        self.runtime.access_snapshot(&self.tree.commands())
    }

    fn find(&self, role: Role, name: &str) -> Option<AccessNode> {
        self.nodes()
            .into_iter()
            .find(|node| node.role == role && node.name == name)
    }

    fn control(&self, role: Role, name: &str) -> AccessNode {
        crate::test_support::control(&self.nodes(), role, name)
    }

    fn send(&mut self, event: Event) {
        route(&mut self.tree, &event);
        settle(&self.tree);
    }

    fn click_at(&mut self, (x, y): (f64, f64)) {
        self.send(press(x, y));
        self.send(release(x, y));
    }

    fn click(&mut self, role: Role, name: &str) {
        self.click_at(centre(self.control(role, name).rect));
    }

    /// The window commands every press since the last call asked for.
    fn asked(&self) -> Vec<WindowCommand> {
        take_window_commands()
            .into_iter()
            .filter(|command| !matches!(command, WindowCommand::SetCursor(_)))
            .collect()
    }

    /// A point of the bar with nothing drawn on it: between the end of the start side's controls and the palette's trigger.
    fn empty_bar(&self) -> (f64, f64) {
        let settings = self.control(Role::Button, "Settings").rect;
        let find = self
            .control(Role::Button, "Find a preview or command in catalogue")
            .rect;
        let x = (settings.x + settings.width + find.x) / 2.0;
        (x as f64, (settings.y + settings.height / 2.0) as f64)
    }
}

fn middle_of(rect: Rect) -> f32 {
    rect.x + rect.width / 2.0
}

#[test]
fn the_palette_trigger_is_centred_on_the_window_rather_than_between_its_neighbours() {
    for width in [WIDE, MEDIUM] {
        let workshop = Workshop::open(Frame::Own, width);
        let find = workshop
            .control(Role::Button, "Find a preview or command in catalogue")
            .rect;
        let off = middle_of(find) - width as f32 / 2.0;
        assert!(
            off.abs() <= 1.0,
            "{width} wide, the trigger is {off} px off centre"
        );
        assert_eq!(find.width, FIND_WIDTH, "{width} wide");
    }
}

#[test]
fn a_window_with_no_room_for_the_field_folds_it_into_a_search_button_before_the_views() {
    let mut workshop = Workshop::open(Frame::Own, 700);
    let name = "Find a preview or command in catalogue";
    let find = workshop.control(Role::Button, name).rect;
    assert!(find.width < FIND_FIELD_COMFORT, "{find:?}");
    let views = workshop.control(Role::Tab, "Canvas").rect;
    assert!(find.x + find.width <= views.x, "{find:?} {views:?}");
    let close = workshop.control(Role::Button, "Close").rect;
    assert!(close.x + close.width <= 700.0, "{close:?}");
    workshop.click(Role::Button, name);
    assert!(workshop.find(Role::Dialog, "Command palette").is_some());
}

#[test]
fn the_trigger_shows_the_project_and_opens_the_palette() {
    let mut workshop = Workshop::open(Frame::Host, WIDE);
    assert!(texts(&workshop.tree).iter().any(|text| text == PROJECT));
    workshop.click(Role::Button, "Find a preview or command in catalogue");
    assert!(workshop.find(Role::Dialog, "Command palette").is_some());
}

#[test]
fn a_project_with_no_name_shows_the_workshop_in_the_trigger() {
    let workshop = Workshop::named(Frame::Host, WIDE, None);
    assert!(texts(&workshop.tree).iter().any(|text| text == "Workshop"));
    assert!(
        workshop
            .find(Role::Button, "Find a preview or command in Workshop")
            .is_some()
    );
}

#[test]
fn the_preview_settings_are_one_menu_however_wide_the_window() {
    let mut workshop = Workshop::open(Frame::Host, WIDE);
    assert!(workshop.find(Role::ComboBox, "Mode System").is_none());
    workshop.click(Role::Button, "Settings");
    crate::test_support::setting_row(&workshop.nodes(), "Direction");
}

#[test]
fn the_views_are_icon_tabs_at_the_end_of_the_bar() {
    let workshop = Workshop::open(Frame::Host, WIDE);
    let canvas = workshop.control(Role::Tab, "Canvas").rect;
    let find = workshop
        .control(Role::Button, "Find a preview or command in catalogue")
        .rect;
    assert!(canvas.x > find.x + find.width);
    for view in ["Canvas", "Matrix", "Docs"] {
        assert!(workshop.find(Role::Tab, view).is_some(), "{view}");
        assert!(
            !texts(&workshop.tree).iter().any(|text| text == view),
            "the {view} tab is drawn as an icon"
        );
    }
}

#[test]
fn a_window_the_workshop_frames_has_its_controls_and_a_framed_page_has_none() {
    let own = Workshop::open(Frame::Own, WIDE);
    for name in ["Minimize", "Maximize", "Close"] {
        assert!(own.find(Role::Button, name).is_some(), "{name}");
    }
    drop(own);

    let page = Workshop::open(Frame::Host, WIDE);
    for name in ["Minimize", "Maximize", "Close"] {
        assert!(page.find(Role::Button, name).is_none(), "{name}");
    }
}

#[test]
fn a_native_build_frames_its_own_window() {
    assert_eq!(Frame::CURRENT, Frame::Own);
}

#[test]
fn the_window_controls_ask_the_platform_for_what_they_name() {
    let mut workshop = Workshop::open(Frame::Own, WIDE);
    workshop.asked();
    workshop.click(Role::Button, "Minimize");
    assert_eq!(workshop.asked(), [WindowCommand::Minimize]);
    workshop.click(Role::Button, "Maximize");
    assert_eq!(workshop.asked(), [WindowCommand::ToggleMaximize]);
    workshop.click(Role::Button, "Close");
    assert_eq!(workshop.asked(), [WindowCommand::Close]);
}

#[test]
fn a_maximized_window_offers_to_restore_itself() {
    let workshop = Workshop::open(Frame::Own, WIDE);
    telar::set_surface_maximized(true);
    settle(&workshop.tree);
    assert!(workshop.find(Role::Button, "Restore").is_some());
    assert!(workshop.find(Role::Button, "Maximize").is_none());
    telar::set_surface_maximized(false);
    settle(&workshop.tree);
    assert!(workshop.find(Role::Button, "Maximize").is_some());
}

#[test]
fn the_workshop_takes_the_decorations_off_only_a_window_it_frames() {
    let own = Workshop::open(Frame::Own, WIDE);
    assert!(take_window_commands().contains(&WindowCommand::SetDecorations(false)));
    drop(own);

    let _page = Workshop::open(Frame::Host, WIDE);
    assert!(
        !take_window_commands()
            .iter()
            .any(|command| matches!(command, WindowCommand::SetDecorations(_)))
    );
}

#[test]
fn dragging_the_bar_by_its_empty_space_moves_the_window() {
    let mut workshop = Workshop::open(Frame::Own, WIDE);
    let (x, y) = workshop.empty_bar();
    workshop.asked();
    workshop.send(press(x, y));
    assert!(workshop.asked().is_empty(), "a press alone moves nothing");
    workshop.send(moved(x + 20.0, y));
    assert_eq!(workshop.asked(), [WindowCommand::Drag]);
}

#[test]
fn dragging_from_a_control_in_the_bar_does_not_move_the_window() {
    let mut workshop = Workshop::open(Frame::Own, WIDE);
    let (x, y) = centre(
        workshop
            .control(Role::Button, "Find a preview or command in catalogue")
            .rect,
    );
    workshop.asked();
    workshop.send(press(x, y));
    workshop.send(moved(x + 20.0, y));
    workshop.send(release(x + 20.0, y));
    assert!(!workshop.asked().contains(&WindowCommand::Drag));
}

#[test]
fn a_double_click_on_the_bar_maximizes_the_window() {
    let mut workshop = Workshop::open(Frame::Own, WIDE);
    let at = workshop.empty_bar();
    workshop.asked();
    workshop.click_at(at);
    assert!(workshop.asked().is_empty(), "one click does nothing");
    workshop.click_at(at);
    assert_eq!(workshop.asked(), [WindowCommand::ToggleMaximize]);
    std::thread::sleep(DOUBLE_CLICK + Duration::from_millis(50));
    workshop.click_at(at);
    assert!(
        workshop.asked().is_empty(),
        "a click long after is a first click again"
    );
}

#[test]
fn a_framed_page_neither_moves_nor_maximizes_from_its_bar() {
    let mut workshop = Workshop::open(Frame::Host, WIDE);
    let (x, y) = workshop.empty_bar();
    workshop.asked();
    workshop.click_at((x, y));
    workshop.click_at((x, y));
    workshop.send(press(x, y));
    workshop.send(moved(x + 20.0, y));
    assert!(workshop.asked().is_empty());
}

#[test]
fn the_borders_of_a_window_the_workshop_frames_resize_it() {
    let mut workshop = Workshop::open(Frame::Own, WIDE);
    workshop.asked();
    workshop.send(press(1.0, HEIGHT as f64 / 2.0));
    assert_eq!(
        workshop.asked(),
        [WindowCommand::DragResize(ResizeEdge::West)]
    );
    drop(workshop);

    let mut page = Workshop::open(Frame::Host, WIDE);
    page.asked();
    page.send(press(1.0, HEIGHT as f64 / 2.0));
    assert!(
        !page
            .asked()
            .iter()
            .any(|command| matches!(command, WindowCommand::DragResize(_)))
    );
}

#[test]
fn the_start_of_the_bar_has_a_preview_label_before_the_settings_button() {
    let mut workshop = Workshop::open(Frame::Host, WIDE);
    assert!(
        texts(&workshop.tree)
            .iter()
            .any(|text| text == "Telar Workshop")
    );
    assert!(workshop.find(Role::Button, "Telar Workshop").is_none());
    workshop.click(Role::Button, "Settings");
    crate::test_support::setting_row(&workshop.nodes(), "Direction");
}

#[test]
fn every_control_of_the_bar_is_as_tall_as_the_window_controls_and_centred_in_the_bar() {
    let workshop = Workshop::open(Frame::Own, 700);
    let controls = [
        (Role::Button, "Settings"),
        (Role::Tab, "Canvas"),
        (Role::Tab, "Matrix"),
        (Role::Tab, "Docs"),
        (Role::Button, "Previews"),
        (Role::Button, "Find a preview or command in catalogue"),
        (Role::Button, "Minimize"),
        (Role::Button, "Maximize"),
        (Role::Button, "Close"),
    ];
    for (role, name) in controls {
        let rect = workshop.control(role, name).rect;
        assert_eq!(
            rect.height,
            telar_components::metrics::control_height(),
            "{name} is {rect:?}"
        );
        assert_eq!(
            rect.y,
            (super::HEIGHT - telar_components::metrics::control_height()) / 2.0,
            "{name} is {rect:?}"
        );
    }
}

#[test]
fn the_palette_field_is_as_tall_as_the_rest_of_the_bar() {
    let workshop = Workshop::open(Frame::Own, WIDE);
    let find = workshop
        .control(Role::Button, "Find a preview or command in catalogue")
        .rect;
    let close = workshop.control(Role::Button, "Close").rect;
    assert_eq!(find.height, close.height);
    assert_eq!(find.y, close.y);
}

/// Where the bar draws `label`: the rect of its line, in the window, and the style it is set in.
fn drawn_text(workshop: &Workshop, label: &str) -> (Rect, telar::TextStyle) {
    let mut found = None;
    telar::for_each_with_matrix(&workshop.tree.commands(), |command, matrix| {
        if let telar::DrawCommand::Text {
            text, rect, style, ..
        } = command
            && text.as_ref() == label
            && found.is_none()
        {
            let at = Rect::new(
                matrix[4] + rect.x,
                matrix[5] + rect.y,
                rect.width,
                rect.height,
            );
            found = Some((at, (**style).clone()));
        }
    });
    found.unwrap_or_else(|| panic!("{label} is not drawn"))
}

/// The middle of the ink of `label` as the bar draws it, top to bottom.
fn ink_middle(workshop: &Workshop, label: &str) -> f32 {
    let (line, style) = drawn_text(workshop, label);
    let (top, height) = telar::measure_ink_bounds(label, line.width, &style);
    line.y + top + height / 2.0
}

/// The controls of the bar left to right, those a narrow window has hidden left out.
fn bar_controls(workshop: &Workshop) -> Vec<AccessNode> {
    let mut controls: Vec<AccessNode> = workshop
        .nodes()
        .into_iter()
        .filter(|node| matches!(node.role, Role::Button | Role::Tab))
        .filter(|node| node.rect.width > 0.0 && node.rect.y < super::HEIGHT)
        .collect();
    controls.sort_by(|a, b| a.rect.x.total_cmp(&b.rect.x));
    controls
}

/// The space between the field and the control before it, and between it and the control after it.
fn gaps_around_the_field(workshop: &Workshop) -> (f32, f32) {
    let field = workshop
        .control(Role::Button, "Find a preview or command in catalogue")
        .rect;
    let controls = bar_controls(workshop);
    let before = controls
        .iter()
        .filter(|node| node.rect.x + node.rect.width <= field.x)
        .map(|node| node.rect.x + node.rect.width)
        .fold(f32::MIN, f32::max);
    let after = controls
        .iter()
        .filter(|node| node.rect.x >= field.x + field.width)
        .map(|node| node.rect.x)
        .fold(f32::MAX, f32::min);
    (field.x - before, after - (field.x + field.width))
}

#[test]
fn a_wide_bar_centres_the_field_on_the_window_in_either_frame() {
    for frame in [Frame::Own, Frame::Host] {
        for width in [WIDE, 1100, MEDIUM] {
            let workshop = Workshop::open(frame, width);
            let field = workshop
                .control(Role::Button, "Find a preview or command in catalogue")
                .rect;
            let off = middle_of(field) - width as f32 / 2.0;
            assert!(off.abs() <= 0.5, "{frame:?} {width}: {off} px off centre");
        }
    }
}

/// Between the width that folds the field into a button and the width that can centre it on the window, the field stands between the two groups: as far from the last control of the start as from the first of the end, never with a gap on one side the other does not have. The pixel rounding leaves when the room is odd is all the two may differ by.
#[test]
fn a_bar_too_narrow_to_centre_the_field_sets_it_as_far_from_either_group() {
    for frame in [Frame::Own, Frame::Host] {
        for width in [760, 800, 850, 899, 900, 910, 920] {
            let workshop = Workshop::open(frame, width);
            let field = workshop
                .control(Role::Button, "Find a preview or command in catalogue")
                .rect;
            if (middle_of(field) - width as f32 / 2.0).abs() <= 0.5 {
                continue;
            }
            let (before, after) = gaps_around_the_field(&workshop);
            assert!(
                (before - after).abs() <= 1.0,
                "{frame:?} {width}: {before} px before the field and {after} px after it"
            );
            assert!(before >= GAP - 0.5, "{frame:?} {width}: {before}");
        }
    }
    let workshop = Workshop::open(Frame::Own, 900);
    let field = workshop
        .control(Role::Button, "Find a preview or command in catalogue")
        .rect;
    assert!(
        (middle_of(field) - 450.0).abs() > 0.5,
        "900 wide, the window's own controls leave no room to centre it"
    );
}

/// The project's name sits in the middle of the field by its ink, beside the icon, as any other text in a control does.
#[test]
fn the_project_name_is_centred_in_the_field_by_its_ink() {
    for width in [WIDE, 800] {
        let workshop = Workshop::open(Frame::Own, width);
        let field = workshop
            .control(Role::Button, "Find a preview or command in catalogue")
            .rect;
        let off = ink_middle(&workshop, PROJECT) - (field.y + field.height / 2.0);
        assert!(
            off.abs() <= 0.5,
            "{width}: the name is {off} px off the field's middle"
        );
    }
}

/// The bold label is set in a control's box, so its text starts where a ghost button's label would and stands on the line a control's label does.
#[test]
fn the_preview_label_is_inset_and_centred_like_a_control_label() {
    let pad = telar_components::metrics::control_pad_x();
    let wide = Workshop::open(Frame::Host, WIDE);
    let (label, _) = drawn_text(&wide, "Telar Workshop");
    assert_eq!(label.x, WORKBENCH_GRID + pad, "first in the bar");
    let (settings, _) = drawn_text(&wide, "Settings");
    assert_eq!(label.y, settings.y, "on the line the menu's label is on");
    drop(wide);

    let narrow = Workshop::open(Frame::Host, 800);
    let opener = narrow.control(Role::Button, "Previews").rect;
    let (label, _) = drawn_text(&narrow, "Telar Workshop");
    assert_eq!(
        label.x,
        opener.x + opener.width + GAP + pad,
        "after the opener"
    );
}

/// The sidebar's opener on a narrow window is an icon, named for what it opens.
#[test]
fn the_sidebar_opener_is_an_icon_button_named_previews() {
    let mut workshop = Workshop::open(Frame::Host, 800);
    let opener = workshop.control(Role::Button, "Previews").rect;
    assert_eq!(opener.width, opener.height, "{opener:?}");
    assert!(
        !texts(&workshop.tree).iter().any(|text| text == "Previews"),
        "the opener draws an icon, not its name"
    );
    workshop.click(Role::Button, "Previews");
    assert!(workshop.find(Role::Dialog, "Previews").is_some());
}

/// The Settings menu's label stands as far from the trigger's left edge as from its right, in either frame and at either width: the trigger is sized from the label in the style it is drawn in, and the line drawn in it is that wide. Within a pixel, as the label and the trigger's edges are each snapped to whole pixels wherever the bar puts the trigger.
#[test]
fn the_settings_label_is_as_far_from_either_edge_of_its_trigger() {
    for frame in [Frame::Own, Frame::Host] {
        for width in [WIDE, 800] {
            let workshop = Workshop::open(frame, width);
            let trigger = workshop.control(Role::Button, "Settings").rect;
            let (line, style) = drawn_text(&workshop, "Settings");
            let (drawn, _) = telar::measure_text("Settings", None, f32::INFINITY, &style);
            let before = line.x - trigger.x;
            let after = trigger.x + trigger.width - (line.x + drawn);
            assert!(
                (before - after).abs() < 1.0,
                "{frame:?} {width}: {before} px before the label and {after} px after it"
            );
        }
    }
}
