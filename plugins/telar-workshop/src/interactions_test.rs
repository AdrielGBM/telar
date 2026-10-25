use std::time::Duration;

use telar::preview::{ArgValue, Frame, PreviewCtx, PreviewEntry, by_role};
use telar::testing::{Scoped, centre, key_with, mount, press, release, route, texts};
use telar::{
    AccessNode, App, AppRuntime, Children, ComponentList, DrawCommand, Event, Key, LayoutError,
    LayoutItem, LocalApp, ModifiersState, NamedKey, Reactive, Rect, RenderNode, Role, Size, signal,
};
use telar_components::{ButtonProps, TooltipProps, button, tooltip};

use super::Verdict;
use super::frame::drawn;
use crate::WorkshopApp;
use crate::test_support::{control, control_matching, settle};

const WIDTH: u32 = 1600;
const HEIGHT: u32 = 1400;
const PASSING: &str = "plays--counter--passing";
const FAILING: &str = "plays--counter--failing";
const PANICKING: &str = "plays--counter--panicking";
const PLAIN: &str = "plays--counter--plain";
const TIPPED: &str = "plays--tipped--clicked";

fn counter(_: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let presses = signal(0_u32);
    button(
        ButtonProps::props()
            .label(Reactive::of(move || format!("Pressed {}", presses.get())))
            .on_press(std::rc::Rc::new(move || {
                presses.update(|count| *count += 1)
            }))
            .build(),
        Children::default(),
    )
}

fn tipped(_: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let go = button(
        ButtonProps::props()
            .label("Go")
            .on_press(std::rc::Rc::new(|| {}))
            .build(),
        Children::default(),
    )?;
    let mut slots = telar::Slots::new();
    slots.push(None, go);
    tooltip(
        TooltipProps::props().text("Explains").build(),
        Children::from(slots),
    )
}

fn entries() -> Vec<PreviewEntry> {
    vec![
        PreviewEntry::new(TIPPED, "tipped", "Clicked", tipped).play(|canvas| {
            canvas.click(by_role(Role::Button).named("Go"))?;
            canvas.expect_text("Explains")
        }),
        PreviewEntry::new(PASSING, "counter", "Passing", counter).play(|canvas| {
            canvas.click(by_role(Role::Button).named("Pressed 0"))?;
            canvas.expect_text("Pressed 1")
        }),
        PreviewEntry::new(FAILING, "counter", "Failing", counter).play(|canvas| {
            canvas.click(by_role(Role::Button).named("Pressed 0"))?;
            canvas.expect_text("Pressed 9")
        }),
        PreviewEntry::new(PANICKING, "counter", "Panicking", counter)
            .play(|_| panic!("the play lost its place")),
        PreviewEntry::new(PLAIN, "counter", "Plain", counter),
    ]
}

const CLICKED: &str = "click button \"Pressed 0\"";

struct Workshop {
    runtime: LocalApp<WorkshopApp>,
    tree: Scoped<ComponentList>,
}

impl Workshop {
    fn open(id: &str) -> Self {
        Self::open_with(id, r#"{"@workshop/panel.size":"700.0"}"#)
    }

    fn open_with(id: &str, hot: &str) -> Self {
        telar::testing::install_test_face();
        telar::hot_restore_json(hot);
        let runtime = LocalApp(WorkshopApp::new(entries()).select(id));
        let tree = mount(|| runtime.0.root(), WIDTH, HEIGHT);
        let mut workshop = Self { runtime, tree };
        workshop.settle();
        workshop.click(Role::Tab, "Interactions");
        workshop
    }

    /// Lets the turn of the loop a canvas waits for pass, so a play due to run does.
    fn next_turn(&mut self) {
        telar::testing::advance_time(Duration::ZERO);
        self.settle();
    }

    fn settle(&self) {
        settle(&self.tree);
    }

    fn draws(&self, text: &str) -> bool {
        texts(&self.tree).iter().any(|drawn| drawn == text)
    }

    fn access(&self) -> Vec<AccessNode> {
        self.runtime.access_snapshot(&self.tree.commands())
    }

    fn control(&self, role: Role, name: &str) -> AccessNode {
        control(&self.access(), role, name)
    }

    fn has(&self, role: Role, name: &str) -> bool {
        self.access()
            .iter()
            .any(|node| node.role == role && node.name == name)
    }

    fn dispatch(&mut self, event: Event) {
        route(&mut self.tree, &event);
        self.settle();
    }

    fn click(&mut self, role: Role, name: &str) {
        let rect = self.control(role, name).rect;
        self.press_at(rect);
    }

    fn press_at(&mut self, rect: Rect) {
        let (x, y) = centre(rect);
        self.dispatch(press(x, y));
        self.dispatch(release(x, y));
    }

    fn key(&mut self, key: Key) {
        self.dispatch(key_with(key, ModifiersState::default()));
    }

    fn step(&self, label: &str) -> AccessNode {
        control_matching(&self.access(), Role::Button, label, |node| {
            node.name.starts_with(label)
        })
    }

    /// Whether the canvas shows the frame `step` left: the region showing it is named after it.
    fn shows_frame_of(&self, step: usize) -> bool {
        let name = format!("The canvas after step {step}");
        self.access()
            .iter()
            .any(|node| node.role != Role::Drawing && node.name.contains(&name))
    }
}

#[test]
fn the_play_runs_on_the_canvas_as_it_mounts_and_its_steps_are_listed_in_order() {
    let mut workshop = Workshop::open(PASSING);
    assert!(
        workshop.draws("The play has not run on this canvas"),
        "nothing has run before the canvas has had its turn"
    );
    workshop.next_turn();
    assert!(workshop.draws("Steps passed: 2"));
    let steps: Vec<String> = workshop
        .access()
        .into_iter()
        .filter(|node| node.role == Role::Button && node.name.starts_with("Step "))
        .map(|node| node.name)
        .collect();
    assert_eq!(
        steps,
        [
            format!("Step 1: {CLICKED}, passed"),
            String::from("Step 2: expect text \"Pressed 1\", passed"),
        ]
    );
    assert!(
        workshop.has(Role::Button, "Pressed 1"),
        "the live canvas is left where the play left it"
    );
    assert!(workshop.draws("2"), "the tab counts the steps");
}

#[test]
fn a_failed_step_is_red_with_why_and_the_frame_it_left() {
    let mut workshop = Workshop::open(FAILING);
    workshop.next_turn();
    assert!(workshop.draws("Failed at step 2 of 2"));
    let failed = workshop.step("Step 2: expect text \"Pressed 9\", failed");
    assert_eq!(failed.toggled, Some(false));
    assert!(
        texts(&workshop.tree)
            .iter()
            .any(|text| text.contains("expected the text \"Pressed 9\"")),
        "the step says why it failed"
    );
    let thumbnail = workshop
        .access()
        .into_iter()
        .find(|node| node.role == Role::Drawing && node.name == "The canvas after step 2")
        .expect("the step shows the frame it left");
    assert!(
        !workshop.shows_frame_of(2),
        "beside the step, not yet on the canvas"
    );
    workshop.press_at(thumbnail.rect);
    assert!(
        workshop.shows_frame_of(2),
        "pressing the frame shows it on the canvas"
    );
    let pressed: Vec<_> = workshop
        .access()
        .into_iter()
        .filter(|node| node.role == Role::Button && node.name == "Pressed 1")
        .collect();
    assert_eq!(
        pressed.len(),
        1,
        "a pictured frame tells a reader of no control: only the live one is there"
    );
}

#[test]
fn a_play_that_panics_fails_a_last_step_of_its_own() {
    let mut workshop = Workshop::open(PANICKING);
    workshop.next_turn();
    assert!(workshop.draws("Failed at step 1 of 1"));
    workshop.step("Step 1: play, failed");
    assert!(workshop.draws("the play lost its place"));
}

#[test]
fn picking_a_step_shows_its_frame_in_place_of_the_live_canvas_until_back_to_live() {
    let mut workshop = Workshop::open(PASSING);
    workshop.next_turn();
    let first = workshop.step("Step 1:").rect;
    workshop.press_at(first);
    assert_eq!(workshop.step("Step 1:").toggled, Some(true));
    assert!(workshop.shows_frame_of(1));
    assert!(workshop.draws(&format!("Step 1 of 2: {CLICKED}")));

    workshop.click(Role::Button, "Back to live");
    assert!(!workshop.shows_frame_of(1));
    assert_eq!(workshop.step("Step 1:").toggled, Some(false));
    assert!(!workshop.has(Role::Button, "Back to live"));
}

#[test]
fn pressing_the_picked_step_again_goes_back_to_live() {
    let mut workshop = Workshop::open(PASSING);
    workshop.next_turn();
    let first = workshop.step("Step 1:").rect;
    workshop.press_at(first);
    assert!(workshop.shows_frame_of(1));
    workshop.press_at(first);
    assert!(!workshop.shows_frame_of(1));
}

#[test]
fn the_arrows_scrub_through_the_steps_and_escape_goes_back_to_live() {
    let mut workshop = Workshop::open(PASSING);
    workshop.next_turn();
    let first = workshop.step("Step 1:").rect;
    workshop.press_at(first);
    workshop.key(Key::Named(NamedKey::ArrowDown));
    assert!(workshop.shows_frame_of(2));
    assert!(workshop.step("Step 2:").focused, "the keyboard follows");
    workshop.key(Key::Named(NamedKey::ArrowDown));
    assert!(workshop.shows_frame_of(2), "the last step stays picked");
    workshop.key(Key::Named(NamedKey::Home));
    assert!(workshop.shows_frame_of(1));
    workshop.key(Key::Named(NamedKey::End));
    assert!(workshop.shows_frame_of(2));
    workshop.key(Key::Named(NamedKey::Escape));
    assert!(!workshop.shows_frame_of(2));
    assert!(!workshop.shows_frame_of(1));
}

#[test]
fn with_auto_run_off_a_mount_waits_and_lists_the_last_steps_as_pending_until_replayed() {
    let mut workshop = Workshop::open(PASSING);
    workshop.next_turn();
    workshop.click(Role::Switch, "Run on mount");
    workshop.click(Role::Button, "Remount");
    workshop.next_turn();
    assert!(workshop.has(Role::Button, "Pressed 0"), "nothing ran");
    assert!(workshop.has(Role::ListItem, &format!("Step 1: {CLICKED}, not run yet")));
    assert!(workshop.draws("The play has not run on this canvas"));

    workshop.click(Role::Button, "Replay");
    workshop.next_turn();
    assert!(workshop.draws("Steps passed: 2"));
    assert!(workshop.has(Role::Button, "Pressed 1"));
}

#[test]
fn replay_runs_the_play_again_on_a_fresh_canvas() {
    let mut workshop = Workshop::open(PASSING);
    workshop.next_turn();
    workshop.click(Role::Button, "Replay");
    workshop.next_turn();
    assert!(
        workshop.draws("Steps passed: 2"),
        "a fresh canvas starts at Pressed 0 again, which the play's first click needs"
    );
    assert!(workshop.has(Role::Button, "Pressed 1"));
}

#[test]
fn whether_a_play_runs_on_mount_survives_a_hot_reload() {
    let mut workshop = Workshop::open(PASSING);
    workshop.click(Role::Switch, "Run on mount");
    let snapshot = telar::hot_snapshot_json();
    drop(workshop);
    let mut workshop = Workshop::open_with(PASSING, &snapshot);
    assert_eq!(
        workshop.control(Role::Switch, "Run on mount").toggled,
        Some(false)
    );
    workshop.next_turn();
    assert!(workshop.has(Role::Button, "Pressed 0"));
}

#[test]
fn a_preview_without_a_play_says_how_to_give_it_one() {
    let mut workshop = Workshop::open(PLAIN);
    workshop.next_turn();
    assert!(workshop.draws("This preview has no play"));
    assert!(
        texts(&workshop.tree)
            .iter()
            .any(|drawn| drawn.starts_with("[play]\n")),
        "with how to give it one in .rsx"
    );
}

#[test]
fn away_from_the_canvas_the_panel_offers_to_show_it_and_runs_the_play_there() {
    let mut workshop = Workshop::open(PASSING);
    workshop.click(Role::Tab, "Matrix");
    workshop.click(Role::Button, "Unfold the panels");
    assert!(workshop.draws("The play runs on the canvas"));
    workshop.click(Role::Button, "Show the canvas");
    workshop.next_turn();
    assert!(workshop.draws("Steps passed: 2"));
}

#[test]
fn a_frame_is_drawn_whole_and_centred_in_the_box_it_is_given() {
    let frame = Frame::new(
        vec![DrawCommand::PopElement, DrawCommand::PopSurface],
        Size::new(100.0, 50.0),
    );
    let RenderNode::Clip { rect, children, .. } = drawn(&frame, Rect::new(0.0, 0.0, 200.0, 200.0))
    else {
        panic!("a frame is clipped to its size");
    };
    assert_eq!(rect, Rect::new(0.0, 50.0, 200.0, 100.0));
    let [RenderNode::Transform { matrix, children }] = &children[..] else {
        panic!("a frame is placed by one transform");
    };
    assert_eq!(*matrix, [2.0, 0.0, 0.0, 2.0, 0.0, 50.0]);
    assert!(children.is_empty(), "markers are left out");
    assert!(matches!(
        drawn(
            &Frame::new(Vec::new(), Size::ZERO),
            Rect::new(0.0, 0.0, 10.0, 10.0)
        ),
        RenderNode::Empty
    ));
}

#[test]
fn whether_a_play_runs_on_mount_is_kept_between_runs_of_the_workshop() {
    use std::sync::Arc;

    use telar::MemoryStore;
    use telar::preview::host::PreviewRequest;

    use crate::state::WorkshopState;
    use crate::store::{Keeper, keys};

    let store = Arc::new(MemoryStore::default());
    let keeper = Keeper::in_store(store.clone());
    let state = WorkshopState::kept_in(entries().into(), &PreviewRequest::default(), &keeper, None);
    assert!(
        state.interactions().auto_run().get(),
        "a play runs on mount at first"
    );
    state.interactions().auto_run().set(false);
    keeper.flush();
    assert_eq!(
        telar::PreferenceStore::get(&*store, keys::PLAY_AUTO_RUN).as_deref(),
        Some("false")
    );

    telar::hot_restore_json("{}");
    let next = WorkshopState::kept_in(
        entries().into(),
        &PreviewRequest::default(),
        &Keeper::in_store(store),
        None,
    );
    assert!(!next.interactions().auto_run().get());
}

const STEPPER: &str = "plays--stepper--default";

fn stepper(ctx: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let count = ctx.signal("count", 0_i64);
    button(
        ButtonProps::props()
            .label(Reactive::of(move || format!("Count {}", count.get())))
            .on_press(std::rc::Rc::new(move || count.update(|count| *count += 1)))
            .build(),
        Children::default(),
    )
}

fn stepping_entries() -> Vec<PreviewEntry> {
    vec![
        PreviewEntry::new(STEPPER, "stepper", "Default", stepper).play(|canvas| {
            canvas.click(by_role(Role::Button).named("Count 0"))?;
            canvas.expect_text("Count 1")
        }),
    ]
}

struct Stepping {
    runtime: LocalApp<crate::test_support::Shell>,
    tree: Scoped<ComponentList>,
}

impl Stepping {
    fn open() -> Self {
        telar::reset_layout_runtime();
        telar::focus::clear();
        telar::testing::install_test_face();
        let request = telar::preview::host::PreviewRequest::new(Some(STEPPER), None);
        let state = crate::state::WorkshopState::new(stepping_entries().into(), &request);
        let runtime = LocalApp(crate::test_support::Shell { state });
        let tree = mount(|| runtime.0.root(), WIDTH, HEIGHT);
        let mut stepping = Self { runtime, tree };
        stepping.next_turn();
        stepping
    }

    fn state(&self) -> &crate::state::WorkshopState {
        &self.runtime.0.state
    }

    fn next_turn(&mut self) {
        settle(&self.tree);
        telar::testing::advance_time(Duration::ZERO);
        settle(&self.tree);
    }

    fn remount(&mut self) {
        self.state().remount();
        self.next_turn();
    }

    fn count(&self) -> ArgValue {
        let entry = self.state().selected().expect("a preview is selected");
        self.state()
            .args(&entry)
            .get("count")
            .expect("the arg is read")
    }

    fn verdict(&self) -> Verdict {
        self.state()
            .interactions()
            .run()
            .expect("the play ran")
            .verdict()
    }

    fn drawn(&self, text: &str) -> bool {
        texts(&self.tree).iter().any(|drawn| drawn == text)
    }
}

#[test]
fn a_play_passes_on_every_remount_because_each_starts_from_the_same_args() {
    let mut stepping = Stepping::open();
    assert_eq!(stepping.verdict(), Verdict::Passed);
    for _ in 0..3 {
        stepping.remount();
        assert_eq!(stepping.verdict(), Verdict::Passed);
        assert_eq!(stepping.count(), ArgValue::Int(1));
    }
}

#[test]
fn what_a_play_changes_stays_until_the_next_remount_and_is_no_edit() {
    let mut stepping = Stepping::open();
    assert!(stepping.drawn("Count 1"), "the play's effect stays visible");
    let entry = stepping.state().selected().expect("a preview is selected");
    let args = stepping.state().args(&entry);
    assert!(args.overrides().is_empty(), "a play's write is not an edit");
    assert!(!args.states().iter().any(|arg| arg.edited));

    stepping.state().interactions().auto_run().set(false);
    stepping.remount();
    assert!(stepping.drawn("Count 0"), "a remount restores the defaults");
    assert_eq!(stepping.count(), ArgValue::Int(0));
}

#[test]
fn a_users_edit_survives_remounts_and_the_play_starts_from_it() {
    let mut stepping = Stepping::open();
    let entry = stepping.state().selected().expect("a preview is selected");
    let args = stepping.state().args(&entry);
    stepping.state().interactions().auto_run().set(false);
    stepping.remount();
    args.set("count", ArgValue::Int(5))
        .expect("the arg takes it");
    stepping.next_turn();
    assert!(stepping.drawn("Count 5"));

    stepping.remount();
    assert!(stepping.drawn("Count 5"), "a remount keeps the user's edit");
    assert_eq!(
        args.overrides(),
        [(String::from("count"), ArgValue::Int(5))]
    );

    crate::interactions::replay(stepping.state());
    stepping.next_turn();
    assert_eq!(
        stepping.count(),
        ArgValue::Int(5),
        "the failed click found no `Count 0`"
    );
    assert!(matches!(stepping.verdict(), Verdict::Failed(_)));
    stepping.remount();
    assert!(stepping.drawn("Count 5"));
}

#[test]
fn a_play_that_ran_as_the_canvas_mounted_leaves_nothing_hovered_behind() {
    let mut workshop = Workshop::open(TIPPED);
    workshop.next_turn();
    assert!(
        workshop.draws("Steps passed: 2"),
        "the play saw the tooltip open"
    );
    assert!(
        !workshop.draws("Explains"),
        "the play's pointer is not the person's, so what it hovered is let go"
    );
}
