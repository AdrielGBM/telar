use std::cell::Cell;
use std::rc::Rc;

use super::*;
use crate::app::App;
use crate::app_runtime::LocalApp;
use crate::runner::host::RenderChannels;
use platform_headless::HeadlessWindow;

/// An app whose content never changes — a shell's frame ring, a wallpaper, a static diagram. Its tree's own generation is fixed for the life of the tree, which is what makes the collision below reachable.
struct Unchanging;

impl App for Unchanging {
    fn root(&self) -> Box<dyn ui_tree::Component> {
        ui_core::reset_layout_runtime();
        Box::new(
            ui_core::Rectangle::new(
                layout_core::LayoutStyle::new().width(10.0).height(10.0),
                || renderer_core::RectStyle::filled(renderer_core::Color::BLACK, 0.0),
            )
            .expect("a rectangle builds"),
        )
    }
}

fn handler() -> AppHandler<HeadlessWindow, ()> {
    build_app_handler::<HeadlessWindow, ()>(
        Box::new(LocalApp(Unchanging)),
        Arc::new(services_core::NoPaths),
        crate::runner::font_config::FontSetup::default(),
        RendererBackend::Software,
        UserPrefs::default(),
        "generation-test".to_string(),
        SurfaceRenderer::builtin(),
    )
}

/// The keepalive exists so the next frame does not wait for the GPU to clock back up, and only somebody who is there can ask for a next frame. What it used to key on was a timer since the last *frame*, which measures the wrong thing: a screen being read produces no frames at all, so it slept three seconds into somebody looking straight at it.
#[test]
fn the_gpu_is_held_awake_for_whoever_is_there_to_notice() {
    let mut handler = handler();
    handler.pacer.renderer_keepalive = true;

    assert!(
        handler.keepalive_due(),
        "a focused window keeps the GPU warm however long it has been still"
    );

    handler.pacer.focused = false;
    assert!(
        !handler.keepalive_due(),
        "a window nobody is looking at will not be typed into"
    );

    handler.pacer.focused = true;
    handler.pacer.last_input = web_time::Instant::now() - IDLE_GRACE - FRAME_BUDGET;
    assert!(
        !handler.keepalive_due(),
        "a window left focused and abandoned sleeps on the backstop"
    );
}

/// A software renderer has no GPU to hold awake, so re-rasterising an unchanged frame buys nothing.
#[test]
fn the_rasteriser_never_asks_for_an_idle_frame() {
    let mut handler = handler();
    handler.pacer.renderer_keepalive = false;
    assert!(
        !handler.keepalive_due(),
        "a rasterising run draws on demand, never on a timer"
    );
}

/// An app whose one rectangle takes its colour from a signal: the smallest thing that can change without anything animating, which is the case the generation got wrong.
struct Tinted(reactive_core::RwSignal<f32>);

impl App for Tinted {
    fn root(&self) -> Box<dyn ui_tree::Component> {
        ui_core::reset_layout_runtime();
        let tint = self.0;
        Box::new(
            ui_core::Rectangle::new(
                layout_core::LayoutStyle::new().width(10.0).height(10.0),
                move || {
                    renderer_core::RectStyle::filled(
                        renderer_core::Color::rgba(tint.get(), 0.0, 0.0, 1.0),
                        0.0,
                    )
                },
            )
            .expect("a rectangle builds"),
        )
    }
}

/// A tree that changed must not ship its new commands under the old number.
///
/// The renderer's whole contract is that equal generations mean identical commands, and it acts on it: the hardware backend skips its pipeline and re-presents the frame it retained. Reading the counter before composing broke it in the one case nothing else covers — a still screen, where no animation is moving the number anyway. On screen it looked like a menu answering a key press a second late, because the correct frame was discarded and the next keepalive blit was what finally drew it.
#[test]
fn new_commands_never_go_out_under_the_previous_generation() {
    let tint = reactive_core::signal(0.0f32);
    let mut handler = build_app_handler::<HeadlessWindow, ()>(
        Box::new(LocalApp(Tinted(tint))),
        Arc::new(services_core::NoPaths),
        crate::runner::font_config::FontSetup::default(),
        RendererBackend::Software,
        UserPrefs::default(),
        "generation-test".to_string(),
        SurfaceRenderer::builtin(),
    );
    handler.tree = Some(handler.app.mount());

    let first = handler.frame_generation();
    assert_eq!(
        handler.frame_generation(),
        first,
        "a frame nothing changed for must keep the number, or the renderer redraws for nothing"
    );

    tint.set(0.5);
    assert_ne!(
        handler.frame_generation(),
        first,
        "the changed frame reused the previous number, so the renderer re-presents the old one"
    );
}

/// A rebuilt tree must never hand the renderer a generation it has already drawn — see [`FrameGeneration`] for why one otherwise would.
///
/// What it looked like: a shell's config reload moved the space its bars reserved but left the frame ring and the wallpaper exactly as they were, until the process was restarted. The bars followed the edit, because a ticking clock had already carried their counter past the collision.
#[test]
fn a_remounted_tree_never_reuses_a_generation_the_renderer_has_drawn() {
    let mut handler = handler();
    let window = HeadlessWindow::new(120, 80);

    handler.tree = Some(handler.app.mount());
    let first = handler.frame_generation();

    handler.remount(&window);
    let composed = handler.tree.as_ref().map(|t| t.generation()).unwrap_or(0);
    let second = handler.frame_generation();

    assert!(
        second > first,
        "a rebuilt tree reported generation {second} after {first} was already drawn, so the renderer \
         would blit the frame from before the rebuild"
    );
    assert!(
        composed <= first,
        "this test proves nothing unless the new tree's own counter really is back in drawn territory: \
         it reported {composed} against {first}"
    );

    // A tree that is not rebuilt keeps its generation, which is what lets the renderer skip idle frames.
    assert_eq!(
        handler.frame_generation(),
        second,
        "an unchanged tree must keep reporting the same generation, or every idle frame re-renders"
    );
}

/// A region filled from outside must not be caught by the idle-frame fast path.
///
/// The trap is that everything looks right: the application renders into its texture at its own pace and Telar schedules frames for it, but the draw commands pointing at that texture are identical every time — the id addresses the view, not its contents, deliberately. Equal generations then tell the renderer it may re-present what it retained, and the window shows one frozen frame while the application keeps repainting behind it at full speed.
#[test]
fn a_continuous_region_moves_the_generation_though_its_commands_never_change() {
    let mut handler = handler();
    let window = HeadlessWindow::new(120, 80);
    handler.tree = Some(handler.app.mount());

    let at_rest = handler.frame_generation();
    assert_eq!(
        handler.frame_generation(),
        at_rest,
        "this app's commands are fixed, so without a region nothing should move"
    );

    let region = motion_core::Continuous::new();
    let first = handler.frame_generation();
    let second = handler.frame_generation();
    assert!(
        first > at_rest && second > first,
        "the generation stalled at {at_rest}/{first}/{second}, so the renderer would blit a stale frame"
    );

    drop(region);
    let after = handler.frame_generation();
    assert_eq!(
        handler.frame_generation(),
        after,
        "with the region gone the surface must go back to skipping idle frames"
    );
    let _ = window;
}

/// A continuous region has to survive **three** gates, and the third is the one that bites hardest.
///
/// `about_to_wait` must keep scheduling frames, `frame_generation` must keep moving so the renderer cannot re-present what it retained — and this one: a frame whose tree is clean falls through to the keepalive branch, which runs at **1 fps**. A region that cleared the first two and not this one is composed once a second while the application refills it at sixty, which reads as a renderer that is merely slow.
#[test]
fn a_clean_tree_with_a_continuous_region_is_still_worth_a_frame() {
    let mut handler = handler();
    let window = HeadlessWindow::new(120, 80);
    assert!(handler.on_resume(&window), "a headless resume builds one");
    // The platform opens a batch before dispatching and `on_redraw` closes and reopens it, so without one open it would close a batch never begun.
    begin_batch();

    // Forced open before each pass: this is about what counts as content, not about the frame clock.
    let opened = || web_time::Instant::now() - FRAME_BUDGET * 2;

    handler.pacer.last_tick = opened();
    handler.on_redraw(&window);
    let first = handler.pacer.last_submit;

    handler.pacer.last_tick = opened();
    handler.on_redraw(&window);
    assert_eq!(
        handler.pacer.last_submit, first,
        "a clean tree with nothing else to say must not submit a frame"
    );

    let _awake = motion_core::Continuous::new();
    handler.pacer.last_tick = opened();
    handler.on_redraw(&window);
    assert!(
        handler.pacer.last_submit > first,
        "the region says the picture changed even though the tree cannot, so this frame had to go out"
    );
}

fn run_a_pass(handler: &mut AppHandler<HeadlessWindow, ()>, window: &HeadlessWindow) {
    handler.new_events();
    handler.pacer.last_tick = web_time::Instant::now() - FRAME_BUDGET * 2;
    handler.on_redraw(window);
}

fn redraw_inside_the_budget(handler: &mut AppHandler<HeadlessWindow, ()>, window: &HeadlessWindow) {
    handler.new_events();
    let ticked = web_time::Instant::now();
    handler.pacer.last_tick = ticked;
    handler.on_redraw(window);
    assert_eq!(
        handler.pacer.last_tick, ticked,
        "precondition: the redraw landed inside the frame budget, so no pass ran"
    );
}

fn resumed_and_settled() -> (AppHandler<HeadlessWindow, ()>, HeadlessWindow) {
    let mut handler = handler();
    let window = HeadlessWindow::new(120, 80);
    handler.new_events();
    assert!(handler.on_resume(&window), "a headless resume builds one");
    handler.about_to_wait();
    run_a_pass(&mut handler, &window);
    assert_eq!(
        handler.about_to_wait(),
        None,
        "precondition: a settled tree with no renderer to keep warm has nothing to wake for"
    );
    (handler, window)
}

#[test]
fn a_redraw_declined_by_the_frame_budget_leaves_a_deadline_within_it() {
    let (mut handler, window) = resumed_and_settled();

    redraw_inside_the_budget(&mut handler, &window);
    let remaining = FRAME_BUDGET.saturating_sub(handler.pacer.last_tick.elapsed());
    let deadline = handler.about_to_wait();

    assert!(
        deadline.is_some_and(|d| d <= remaining),
        "a declined redraw must be scheduled no later than the {remaining:?} left of the budget, got {deadline:?}"
    );
}

#[test]
fn a_pass_that_runs_with_nothing_due_lets_the_loop_sleep_again() {
    let (mut handler, window) = resumed_and_settled();
    redraw_inside_the_budget(&mut handler, &window);
    assert!(
        handler.about_to_wait().is_some(),
        "precondition: a frame is owed"
    );

    run_a_pass(&mut handler, &window);

    assert_eq!(
        handler.about_to_wait(),
        None,
        "the pass paid off the owed frame, so reporting another deadline would spin the loop"
    );
}

struct BackgroundHost {
    landed: Rc<Cell<bool>>,
    collected: Rc<Cell<bool>>,
}

impl RendererHost<HeadlessWindow> for BackgroundHost {
    fn start(&mut self, _window: &HeadlessWindow, _req: &RendererRequest<'_>) -> RendererStart {
        RendererStart::Building
    }

    fn poll(&mut self) -> Option<RendererStart> {
        if !self.landed.get() || self.collected.get() {
            return None;
        }
        self.collected.set(true);
        Some(RendererStart::Started {
            keepalive: false,
            label: "landed",
        })
    }

    fn channels(&self) -> Option<&RenderChannels> {
        None
    }

    fn suspend(&mut self) {}

    fn retire(&mut self) {}
}

#[test]
fn a_pending_build_sleeps_until_its_wake_is_owed_and_the_next_pass_collects_it() {
    let landed = Rc::new(Cell::new(false));
    let collected = Rc::new(Cell::new(false));
    let mut handler = handler();
    handler.renderer_host = Box::new(BackgroundHost {
        landed: Rc::clone(&landed),
        collected: Rc::clone(&collected),
    });
    let window = HeadlessWindow::new(120, 80);
    handler.tree = Some(handler.app.mount());

    run_a_pass(&mut handler, &window);
    assert_eq!(
        handler.about_to_wait(),
        None,
        "a pending build with nothing owed must let the loop sleep; its own wake is what ends the wait"
    );

    landed.set(true);
    redraw_inside_the_budget(&mut handler, &window);
    assert!(
        handler.pacer.frame_owed,
        "the builder's wake landed inside the budget, so its frame is owed"
    );
    assert!(
        handler.about_to_wait().is_some(),
        "an owed frame must leave a deadline, or the renderer is never collected"
    );

    run_a_pass(&mut handler, &window);
    assert!(
        collected.get(),
        "the pass after the budget takes the renderer"
    );
    assert_eq!(
        handler.about_to_wait(),
        None,
        "installed, with nothing due, the loop sleeps"
    );
}

/// The keyboard registry is wired into the runner, not just into `ui-core`.
///
/// Its own unit tests drive `observe` directly, so they would pass just as well with the runner never calling it — and a modifier state nobody feeds is worse than none, because it answers confidently with whatever it last saw.
#[test]
fn the_runner_feeds_the_keyboard_registry() {
    ui_core::reset_keyboard();
    let mut handler = handler();
    let window = HeadlessWindow::new(120, 80);

    assert!(
        !ui_core::modifiers().is_shift,
        "shift is not down before the event"
    );
    handler.on_event(
        Event::ModifiersChanged {
            modifiers: platform_core::ModifiersState {
                is_shift: true,
                ..Default::default()
            },
        },
        &window,
    );
    assert!(
        ui_core::modifiers().is_shift,
        "a bare Shift must reach the registry, since it maps to no Key at all"
    );

    handler.on_event(
        Event::KeyPressed {
            key: platform_core::Key::Named(platform_core::NamedKey::ArrowUp),
            modifiers: Default::default(),
        },
        &window,
    );
    let up = platform_core::Key::Named(platform_core::NamedKey::ArrowUp);
    assert!(
        ui_core::key_held(&up),
        "the runner handed the press to the keyboard registry"
    );
    assert!(
        ui_core::key_pressed(&up),
        "and it reads as pressed this frame"
    );
}

/// The same for the pointer's buttons, and for the same reason: a drag handler asks which button is doing the dragging, and a registry nobody feeds answers confidently with nothing.
#[test]
fn the_runner_feeds_the_pointer_button_registry() {
    ui_core::reset_pointer();
    let mut handler = handler();
    let window = HeadlessWindow::new(120, 80);

    assert!(
        !ui_core::pointer_buttons().any(),
        "no button is down before the event"
    );
    handler.on_event(
        Event::PointerPressed {
            x: 10.0,
            y: 10.0,
            button: platform_core::PointerButton::Secondary,
            source: platform_core::PointerSource::Mouse,
        },
        &window,
    );
    assert!(
        ui_core::pointer_buttons().secondary,
        "the runner recorded which button went down"
    );
    handler.on_event(
        Event::PointerReleased {
            x: 10.0,
            y: 10.0,
            button: platform_core::PointerButton::Secondary,
            source: platform_core::PointerSource::Mouse,
        },
        &window,
    );
    assert!(
        !ui_core::pointer_buttons().any(),
        "and the release cleared it"
    );
}

/// An app whose tree owns state the app never sees — a tint created inside `root` and a text field — beside a tint the app holds. A rebuilt tree is told apart from a kept one by what it forgot.
struct Stateful {
    held: reactive_core::RwSignal<f32>,
    local_start: f32,
    roots: Rc<Cell<u32>>,
    local: Rc<Cell<Option<reactive_core::RwSignal<f32>>>>,
    field: Rc<Cell<Option<ui_core::focus::FocusId>>>,
}

impl Stateful {
    fn new(held: reactive_core::RwSignal<f32>, local_start: f32) -> Self {
        Self {
            held,
            local_start,
            roots: Rc::default(),
            local: Rc::default(),
            field: Rc::default(),
        }
    }
}

impl App for Stateful {
    fn root(&self) -> Box<dyn ui_tree::Component> {
        ui_core::reset_layout_runtime();
        self.roots.set(self.roots.get() + 1);
        let local = reactive_core::signal(self.local_start);
        self.local.set(Some(local));
        let held = self.held;
        let tint = ui_core::Rectangle::new(
            layout_core::LayoutStyle::new().width(40.0).height(40.0),
            move || {
                renderer_core::RectStyle::filled(
                    renderer_core::Color::rgba(local.get(), held.get(), 0.0, 1.0),
                    0.0,
                )
            },
        )
        .expect("a rectangle builds");
        // Drawn in a colour that does not show, so a blinking caret cannot make two frames of the same state differ.
        let field = ui_core::Input::new(
            reactive_core::signal(String::new()),
            layout_core::LayoutStyle::new().width(40.0).height(16.0),
            || renderer_core::TextStyle::new(12.0, renderer_core::Color::TRANSPARENT),
        )
        .expect("a field builds");
        self.field.set(Some(field.focus_id()));
        Box::new(
            ui_core::Container::new(
                layout_core::LayoutStyle::new(),
                vec![ui_core::box_item(tint), ui_core::box_item(field)],
            )
            .expect("a container builds"),
        )
    }
}

#[derive(Clone, Default)]
struct RendererLog {
    frames_per_renderer: Rc<std::cell::RefCell<Vec<u32>>>,
    alive: Rc<Cell<u32>>,
}

struct LoggedRenderer {
    inner: Box<dyn RenderBackend>,
    index: usize,
    log: RendererLog,
}

impl RenderBackend for LoggedRenderer {
    fn begin_frame(
        &mut self,
        width: u32,
        height: u32,
        scale_factor: f32,
        generation: u64,
    ) -> Result<(), renderer_core::RendererError> {
        self.inner
            .begin_frame(width, height, scale_factor, generation)
    }

    fn render_frame(
        &mut self,
        commands: &[renderer_core::DrawCommand],
        clear_color: Option<renderer_core::Color>,
    ) -> Result<(), renderer_core::RendererError> {
        self.log.frames_per_renderer.borrow_mut()[self.index] += 1;
        self.inner.render_frame(commands, clear_color)
    }

    fn read_rgba(&self) -> Option<Vec<u8>> {
        self.inner.read_rgba()
    }
}

impl Drop for LoggedRenderer {
    fn drop(&mut self) {
        self.log.alive.set(self.log.alive.get() - 1);
    }
}

thread_local! {
    static FREED_MEMORY_RETURNS: Cell<u32> = const { Cell::new(0) };
}

fn count_freed_memory_return() {
    FREED_MEMORY_RETURNS.with(|returns| returns.set(returns.get() + 1));
}

fn freed_memory_returns() -> u32 {
    FREED_MEMORY_RETURNS.with(Cell::get)
}

/// The built-in host, with its offscreen renderers logged and its release of freed memory counted rather than performed.
struct LoggedHost {
    inner: crate::runner::host::BuiltinHost<HeadlessWindow>,
    log: RendererLog,
}

impl LoggedHost {
    fn new(log: RendererLog) -> Self {
        Self {
            inner: crate::runner::host::BuiltinHost::new()
                .returning_freed_memory_with(count_freed_memory_return),
            log,
        }
    }
}

impl RendererHost<HeadlessWindow> for LoggedHost {
    fn start(&mut self, window: &HeadlessWindow, req: &RendererRequest<'_>) -> RendererStart {
        self.inner.start(window, req)
    }

    fn channels(&self) -> Option<&RenderChannels> {
        self.inner.channels()
    }

    fn suspend(&mut self) {
        self.inner.suspend();
    }

    fn retire(&mut self) {
        self.inner.retire();
    }

    fn build_offscreen(
        &mut self,
        window: &HeadlessWindow,
        req: &RendererRequest<'_>,
    ) -> Option<Box<dyn RenderBackend>> {
        let inner = self.inner.build_offscreen(window, req)?;
        let index = {
            let mut frames = self.log.frames_per_renderer.borrow_mut();
            frames.push(0);
            frames.len() - 1
        };
        self.log.alive.set(self.log.alive.get() + 1);
        Some(Box::new(LoggedRenderer {
            inner,
            index,
            log: self.log.clone(),
        }))
    }
}

fn stateful_handler(app: Stateful, log: RendererLog) -> AppHandler<HeadlessWindow, ()> {
    let mut handler = build_app_handler::<HeadlessWindow, ()>(
        Box::new(LocalApp(app)),
        Arc::new(services_core::NoPaths),
        crate::runner::font_config::FontSetup::default(),
        RendererBackend::Software,
        UserPrefs::default(),
        "presentation-test".to_string(),
        SurfaceRenderer::builtin(),
    );
    handler.renderer_host = Box::new(LoggedHost::new(log));
    handler
}

fn node_ids(handler: &AppHandler<HeadlessWindow, ()>) -> Vec<u64> {
    let mut nodes = Vec::new();
    if let Some(tree) = &handler.tree {
        tree.walk(&mut nodes);
    }
    nodes.into_iter().map(|node| node.id).collect()
}

fn resume(handler: &mut AppHandler<HeadlessWindow, ()>, window: &HeadlessWindow) {
    handler.new_events();
    assert!(handler.on_resume(window), "a headless resume builds one");
    handler.about_to_wait();
}

/// Hiding a window gives back what presents it and nothing else; showing it again draws the very same tree, whole.
#[test]
fn a_suspended_window_keeps_its_tree_and_presents_it_whole_on_resume() {
    let held = reactive_core::signal(0.25f32);
    let app = Stateful::new(held, 0.0);
    let (roots, local, field) = (
        Rc::clone(&app.roots),
        Rc::clone(&app.local),
        Rc::clone(&app.field),
    );
    let log = RendererLog::default();
    let mut handler = stateful_handler(app, log.clone());
    let window = HeadlessWindow::new(120, 80);

    resume(&mut handler, &window);
    let local = local.get().expect("the tree made its signal");
    let field = field.get().expect("the tree made its field");
    local.set(0.75);
    ui_core::focus::request(field);
    run_a_pass(&mut handler, &window);
    handler.about_to_wait();
    let ids = node_ids(&handler);
    assert_eq!(roots.get(), 1, "precondition: one tree built");

    let returned_before_suspend = freed_memory_returns();
    handler.on_suspend();
    assert!(
        handler.renderer.is_none() && log.alive.get() == 0,
        "a suspended window must hold no renderer, and with it no pixmap"
    );
    assert!(
        freed_memory_returns() > returned_before_suspend,
        "the rasteriser's pages go back to the system once nothing is kept warm"
    );

    held.set(0.5);
    resume(&mut handler, &window);
    assert_eq!(roots.get(), 1, "resume rebuilt the tree");
    assert_eq!(local.get(), 0.75, "the tree's own signal was reset");
    assert_eq!(node_ids(&handler), ids, "the tree's nodes changed identity");
    assert_eq!(
        ui_core::focus::current(),
        Some(field),
        "the field lost the keyboard"
    );

    run_a_pass(&mut handler, &window);
    handler.about_to_wait();
    assert_eq!(
        *log.frames_per_renderer.borrow(),
        vec![1, 1],
        "the first frame after resume is the new renderer's first, so nothing retained went into it"
    );
    let resumed = handler
        .last_frame_rgba()
        .expect("a headless frame reads back");
    handler.on_suspend();
    drop(handler);

    let mut fresh = stateful_handler(Stateful::new(held, 0.75), RendererLog::default());
    resume(&mut fresh, &window);
    run_a_pass(&mut fresh, &window);
    fresh.about_to_wait();
    let fresh_frame = fresh
        .last_frame_rgba()
        .expect("a headless frame reads back");
    assert!(
        resumed == fresh_frame,
        "the frame after resume differs from a fresh window showing the same state"
    );
}

/// A kept tree comes back clean — nothing changed while it was hidden — and still has to be drawn, because the renderer that drew it last is gone.
#[test]
fn a_resumed_window_with_nothing_changed_still_draws() {
    let log = RendererLog::default();
    let mut handler = stateful_handler(Stateful::new(reactive_core::signal(0.0), 0.0), log.clone());
    let window = HeadlessWindow::new(120, 80);
    resume(&mut handler, &window);
    run_a_pass(&mut handler, &window);
    handler.about_to_wait();

    handler.on_suspend();
    resume(&mut handler, &window);
    run_a_pass(&mut handler, &window);
    handler.about_to_wait();

    assert_eq!(
        *log.frames_per_renderer.borrow(),
        vec![1, 1],
        "the resumed window drew nothing, so it would stay blank until something in it changed"
    );
    run_a_pass(&mut handler, &window);
    assert_eq!(
        *log.frames_per_renderer.borrow(),
        vec![1, 1],
        "once drawn, a clean tree goes back to drawing on change only"
    );
}

/// A 120x80 card showing `Grab`, with a 40x40 grip at its top-left showing `EwResize` that drags.
struct Handles;

impl App for Handles {
    fn root(&self) -> Box<dyn ui_tree::Component> {
        ui_core::reset_layout_runtime();
        let grip = ui_core::StyledContainer::new(
            layout_core::LayoutStyle::new().width(40.0).height(40.0),
            |_| renderer_core::RectStyle::default(),
            vec![],
        )
        .expect("a grip builds")
        .cursor(platform_core::Cursor::EwResize)
        .on_drag(|_, _| {});
        let card = ui_core::StyledContainer::new(
            layout_core::LayoutStyle::new()
                .flex_column()
                .width(120.0)
                .height(80.0),
            |_| renderer_core::RectStyle::default(),
            vec![ui_core::box_item(grip)],
        )
        .expect("a card builds")
        .cursor(platform_core::Cursor::Grab);
        Box::new(ui_core::WindowRoot::new(ui_core::box_item(card)))
    }
}

/// The runner forwards a box's cursor request to the window: the innermost hovered box decides, a drag keeps its shape wherever the pointer goes, and leaving hands the shape back.
#[test]
fn the_window_shows_the_cursor_the_box_under_the_pointer_asks_for() {
    use platform_core::{Cursor, PointerButton, PointerSource};

    let mut handler = build_app_handler::<HeadlessWindow, ()>(
        Box::new(LocalApp(Handles)),
        Arc::new(services_core::NoPaths),
        crate::runner::font_config::FontSetup::default(),
        RendererBackend::Software,
        UserPrefs::default(),
        "cursor-test".to_string(),
        SurfaceRenderer::builtin(),
    );
    let window = HeadlessWindow::new(120, 80);
    handler.new_events();
    assert!(handler.on_resume(&window));
    run_a_pass(&mut handler, &window);

    let moved = |x, y| Event::PointerMoved {
        x,
        y,
        source: PointerSource::Mouse,
    };
    handler.on_event(moved(20.0, 20.0), &window);
    handler.on_event(moved(20.0, 20.0), &window);
    assert_eq!(window.cursor(), Cursor::EwResize, "over the grip");
    handler.on_event(moved(100.0, 60.0), &window);
    assert_eq!(window.cursor(), Cursor::Grab, "over the card only");

    handler.on_event(moved(20.0, 20.0), &window);
    handler.on_event(
        Event::PointerPressed {
            x: 20.0,
            y: 20.0,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        },
        &window,
    );
    handler.on_event(moved(300.0, 300.0), &window);
    assert_eq!(
        window.cursor(),
        Cursor::EwResize,
        "a drag keeps its shape outside"
    );
    handler.on_event(
        Event::PointerReleased {
            x: 300.0,
            y: 300.0,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        },
        &window,
    );
    assert_eq!(
        window.cursor(),
        Cursor::Default,
        "and gives it back where it ended"
    );
}

thread_local! {
    static DROPPED_UNDER: Cell<Option<reactive_core::SurfaceHandle>> = const { Cell::new(None) };
}

/// Notes which surface is active when it is dropped.
struct SurfaceWitness;

impl ui_tree::Component for SurfaceWitness {
    fn view(&self) -> ui_tree::RenderNode {
        ui_tree::RenderNode::group([])
    }
}

impl Drop for SurfaceWitness {
    fn drop(&mut self) {
        DROPPED_UNDER.with(|at| at.set(Some(reactive_core::current_surface())));
    }
}

struct Witnessed;

impl App for Witnessed {
    fn root(&self) -> Box<dyn ui_tree::Component> {
        Box::new(SurfaceWitness)
    }
}

/// A tree's widgets withdraw what they registered from their surface's worlds as they drop, so the tree has to go while that surface is still there and active.
#[test]
fn a_dropped_handler_drops_its_tree_inside_its_surface() {
    let mut handler = build_app_handler::<HeadlessWindow, ()>(
        Box::new(LocalApp(Witnessed)),
        Arc::new(services_core::NoPaths),
        crate::runner::font_config::FontSetup::default(),
        RendererBackend::Software,
        UserPrefs::default(),
        "drop-order-test".to_string(),
        SurfaceRenderer::builtin(),
    );
    let surface = ui_core::Surface::new();
    let handle = surface.handle();
    handler.surface = Some(surface);
    {
        let _surface = handler.enter_surface();
        handler.tree = Some(handler.app.mount());
    }

    drop(handler);

    assert_eq!(DROPPED_UNDER.with(Cell::get), Some(handle));
}
