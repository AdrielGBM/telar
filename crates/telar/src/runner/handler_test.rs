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

fn run_a_pass<D: DevOverlay>(handler: &mut AppHandler<HeadlessWindow, D>, window: &HeadlessWindow) {
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

/// A timer is a deadline, not an animation: the loop sleeps until it comes due rather than drawing frames to find out, and the pass at the deadline runs it.
#[test]
fn a_pending_timer_sleeps_the_loop_until_it_comes_due() {
    let (mut handler, window) = resumed_and_settled();
    let delay = std::time::Duration::from_secs(5);
    let fired = Rc::new(Cell::new(false));
    let _timer = reactive_core::run_after(delay, {
        let fired = Rc::clone(&fired);
        move || fired.set(true)
    });

    run_a_pass(&mut handler, &window);
    let wait = handler.about_to_wait();
    assert!(!fired.get());
    assert!(
        wait.is_some_and(|wait| wait > FRAME_BUDGET && wait <= delay),
        "the loop sleeps until the timer, no sooner and no later, got {wait:?}"
    );

    reactive_core::advance_timer_clock(delay);
    run_a_pass(&mut handler, &window);
    assert!(fired.get(), "the pass at the deadline runs it");
    assert_eq!(
        handler.about_to_wait(),
        None,
        "with the timer gone there is nothing to wake for"
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
            unmodified: None,
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

/// A blocking overlay over the bottom half of a 120x80 window that takes every pointer event landing on it.
struct LowerBlanket;

impl ui_tree::OverlaySink for LowerBlanket {
    fn content_rect(&self) -> geometry_core::Rect {
        geometry_core::Rect::new(0.0, 40.0, 120.0, 40.0)
    }

    fn dispatch(&self, _event: &Event) -> EventResult {
        EventResult::Handled
    }
}

/// A move the overlay layer takes skips the tree's hit-testing, not the tree: the box the pointer slid off hears it leave, and a drag under way keeps going.
#[test]
fn a_box_hovered_under_an_overlay_hears_the_pointer_move_onto_it() {
    use platform_core::{Cursor, PointerButton, PointerSource};

    let mut handler = build_app_handler::<HeadlessWindow, ()>(
        Box::new(LocalApp(Handles)),
        Arc::new(services_core::NoPaths),
        crate::runner::font_config::FontSetup::default(),
        RendererBackend::Software,
        UserPrefs::default(),
        "covered-test".to_string(),
        SurfaceRenderer::builtin(),
    );
    let window = HeadlessWindow::new(120, 80);
    handler.new_events();
    assert!(handler.on_resume(&window));
    run_a_pass(&mut handler, &window);
    let blanket = ui_tree::register_overlay(Rc::new(LowerBlanket));

    let moved = |x, y| Event::PointerMoved {
        x,
        y,
        source: PointerSource::Mouse,
    };
    handler.on_event(moved(100.0, 20.0), &window);
    assert_eq!(window.cursor(), Cursor::Grab, "over the card");
    handler.on_event(moved(100.0, 60.0), &window);
    assert_eq!(
        window.cursor(),
        Cursor::Default,
        "the card under the overlay is no longer hovered"
    );

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
    handler.on_event(moved(20.0, 60.0), &window);
    assert_eq!(
        window.cursor(),
        Cursor::EwResize,
        "a drag carried under the overlay keeps its shape"
    );

    ui_tree::unregister_overlay(blanket);
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

#[test]
fn focus_the_surface_took_out_of_every_box_leaves_none_focused() {
    let mut handler = handler();
    let window = HeadlessWindow::new(120, 80);
    let id = ui_core::focus::next_id();
    ui_core::focus::register_as(id, ui_core::focus::FocusKind::Widget);
    ui_core::focus::request(id);

    handler.on_event(Event::FocusChanged { is_focused: false }, &window);
    assert_eq!(
        ui_core::focus::current(),
        Some(id),
        "a window that only lost focus gets its box back on return"
    );
    handler.on_event(Event::FocusLeftBoxes, &window);
    assert_eq!(ui_core::focus::current(), None);
}

/// Back is answered by the app's own history first, and handed back to the platform only when there is nothing left: at the first entry, Android sends the task away.
#[test]
fn back_steps_the_history_until_there_is_none_left_to_step() {
    let at = |path: &str| platform_core::LocationFormat::root().parse(path).unwrap();
    let sink = platform_core::HistorySink::default();
    let mut handler = handler();
    handler.location = Some(super::super::location::LocationBinding::new(Box::new(
        platform_core::FixedLocation::new([at("/"), at("/a")]).recording_into(sink.clone()),
    )));
    handler.hand_over_location();
    let window = HeadlessWindow::new(4, 4);
    handler.new_events();

    assert!(handler.on_back(&window));
    assert_eq!(*sink.lock().unwrap(), [at("/")]);
    assert!(!handler.on_back(&window));
    handler.about_to_wait();
    assert_eq!(*sink.lock().unwrap(), [at("/")]);
}

/// The platform moving by itself is followed and never reported straight back to it.
#[test]
fn a_history_the_platform_reports_is_not_echoed_to_it() {
    let at = |path: &str| platform_core::LocationFormat::root().parse(path).unwrap();
    let sink = platform_core::HistorySink::default();
    let mut handler = handler();
    handler.location = Some(super::super::location::LocationBinding::new(Box::new(
        platform_core::FixedLocation::new([at("/")]).recording_into(sink.clone()),
    )));
    handler.hand_over_location();
    let window = HeadlessWindow::new(4, 4);

    handler.on_event(
        Event::LocationChanged {
            history: vec![at("/"), at("/b")],
        },
        &window,
    );
    assert_eq!(platform_core::location_history(), [at("/"), at("/b")]);
    assert_eq!(
        *sink.lock().unwrap(),
        [at("/")],
        "the platform already stands there"
    );
}

/// A location the hot channel names opens as a link opened into the running app does: on top of where the app is.
#[cfg(all(feature = "dev", not(target_arch = "wasm32")))]
#[test]
fn a_link_from_the_hot_channel_opens_on_top_of_where_the_app_is() {
    let at = |path: &str| platform_core::LocationFormat::root().parse(path).unwrap();
    let mut handler = handler();
    handler.location = Some(super::super::location::LocationBinding::new(Box::new(
        platform_core::FixedLocation::new([at("/preview/a")]),
    )));
    handler.hand_over_location();
    let window = HeadlessWindow::new(4, 4);

    handler.open_link("/preview/b?args=n:2", &window);
    assert_eq!(
        platform_core::location_history(),
        [at("/preview/a"), at("/preview/b?args=n:2")]
    );
}

/// Counts the loop wakes the runner hands the app's runtime.
#[cfg(all(feature = "dev", not(target_arch = "wasm32")))]
struct Woken {
    app: LocalApp<Unchanging>,
    wakers: Rc<Cell<u32>>,
}

#[cfg(all(feature = "dev", not(target_arch = "wasm32")))]
impl AppRuntime for Woken {
    fn mount(&mut self) -> Box<dyn crate::tree::UiTree> {
        self.app.mount()
    }

    fn clear_color(&self) -> Option<renderer_core::Color> {
        self.app.clear_color()
    }

    fn window_config(&self) -> Option<platform_core::WindowConfig> {
        self.app.window_config()
    }

    fn on_frame(&mut self, ctx: &mut platform_core::AppCtx) {
        self.app.on_frame(ctx)
    }

    fn install_task_waker(&self, _waker: platform_core::RedrawWaker) {
        self.wakers.set(self.wakers.get() + 1);
    }
}

#[cfg(all(feature = "dev", not(target_arch = "wasm32")))]
fn woken(wakers: &Rc<Cell<u32>>) -> Box<dyn AppRuntime> {
    Box::new(Woken {
        app: LocalApp(Unchanging),
        wakers: Rc::clone(wakers),
    })
}

/// A reloaded dylib links a reactive-core copy of its own, whose waker slot starts empty: without the wake handed over again, its tasks and timers sit until the next input event.
#[cfg(all(feature = "dev", not(target_arch = "wasm32")))]
#[test]
fn a_runtime_swapped_in_by_a_reload_is_handed_the_loop_wake() {
    let first = Rc::new(Cell::new(0));
    let mut handler = build_app_handler::<HeadlessWindow, ()>(
        woken(&first),
        Arc::new(services_core::NoPaths),
        crate::runner::font_config::FontSetup::default(),
        RendererBackend::Software,
        UserPrefs::default(),
        "reload-waker-test".to_string(),
        SurfaceRenderer::builtin(),
    );
    let window = HeadlessWindow::new(40, 40);
    handler.new_events();
    assert!(handler.on_resume(&window));
    assert_eq!(first.get(), 1, "precondition: the resume hands one over");

    let reloaded = Rc::new(Cell::new(0));
    handler.swap_app(woken(&reloaded), &window);
    assert_eq!(reloaded.get(), 1, "the incoming runtime is handed the wake");
    assert!(handler.tree.is_some(), "and its tree is mounted");
}

/// An app of one `text` naming no family, as `.rsx` writes one.
struct Labelled;

impl App for Labelled {
    fn root(&self) -> Box<dyn ui_tree::Component> {
        ui_core::reset_layout_runtime();
        Box::new(
            ui_core::Text::declaring(
                || "Configured".to_string(),
                layout_core::LayoutStyle::new(),
                |t| t,
            )
            .expect("a text builds"),
        )
    }
}

fn drawn_family(handler: &AppHandler<HeadlessWindow, ()>) -> renderer_core::FontFamily {
    ui_core::relayout_if_dirty();
    let tree = handler.tree.as_ref().expect("a tree is mounted");
    let frame = tree.frame();
    frame
        .iter()
        .find_map(|command| match command {
            renderer_core::DrawCommand::Text { style, .. } => Some(style.font_family.clone()),
            _ => None,
        })
        .expect("the tree drew its text")
}

/// The family a surface is configured with is where its text starts, and the app moves it while the surface runs without building anything again.
#[test]
fn a_surface_opens_in_its_configured_family_and_follows_a_new_one() {
    crate::install_default_text_metrics();
    let mut handler = build_app_handler::<HeadlessWindow, ()>(
        Box::new(LocalApp(Labelled)),
        Arc::new(services_core::NoPaths),
        crate::runner::font_config::FontSetup {
            faces: Vec::new(),
            family: Some("Telar Configured Face".to_string()),
        },
        RendererBackend::Software,
        UserPrefs::default(),
        "font-family-test".to_string(),
        SurfaceRenderer::builtin(),
    );
    let window = HeadlessWindow::new(200, 40);
    handler.remount(&window);
    let configured = renderer_core::FontFamily::from("Telar Configured Face");
    assert_eq!(
        drawn_family(&handler),
        renderer_core::FontFamily::stack([configured, renderer_core::FontFamily::SansSerif])
    );

    ui_core::set_font_family(Some(renderer_core::FontFamily::Monospace));
    assert_eq!(drawn_family(&handler), renderer_core::FontFamily::Monospace);

    handler.remount(&window);
    assert_eq!(
        drawn_family(&handler),
        renderer_core::FontFamily::Monospace,
        "a tree built again keeps what the app chose over what the surface was configured with"
    );
}

/// A tree that counts the pointer presses that reach it.
struct Heard(Rc<Cell<u32>>);

impl ui_tree::Component for Heard {
    fn view(&self) -> ui_tree::RenderNode {
        ui_tree::RenderNode::group([])
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        if matches!(event, Event::PointerPressed { .. }) {
            self.0.set(self.0.get() + 1);
        }
        EventResult::Ignored
    }
}

struct Listening(Rc<Cell<u32>>);

impl App for Listening {
    fn root(&self) -> Box<dyn ui_tree::Component> {
        Box::new(Heard(Rc::clone(&self.0)))
    }
}

/// An overlay that takes every press for itself and lets everything else through.
#[derive(Default)]
struct Shield {
    seen: u32,
}

impl DevOverlay for Shield {
    fn on_frame<'a>(
        &mut self,
        base: &'a [renderer_core::DrawCommand],
        _window_w: f32,
        _window_h: f32,
        _tree_dirty: bool,
    ) -> std::borrow::Cow<'a, [renderer_core::DrawCommand]> {
        std::borrow::Cow::Borrowed(base)
    }

    fn on_event(&mut self, event: &Event) -> ui_tree::OverlayResponse {
        self.seen += 1;
        ui_tree::OverlayResponse {
            consumed: matches!(event, Event::PointerPressed { .. }),
            action: None,
        }
    }
}

fn overlaid<D: DevOverlay>(app: Box<dyn AppRuntime>) -> AppHandler<HeadlessWindow, D> {
    build_app_handler::<HeadlessWindow, D>(
        app,
        Arc::new(services_core::NoPaths),
        crate::runner::font_config::FontSetup::default(),
        RendererBackend::Software,
        UserPrefs::default(),
        "overlay-test".to_string(),
        SurfaceRenderer::builtin(),
    )
}

fn press_at(x: f64, y: f64) -> Event {
    Event::PointerPressed {
        x,
        y,
        button: platform_core::PointerButton::Primary,
        source: platform_core::PointerSource::Mouse,
    }
}

/// The overlay sits over the app, so a press it claims is one the app must never also act on.
#[test]
fn a_press_the_overlay_consumes_never_reaches_the_tree() {
    let heard = Rc::new(Cell::new(0));
    let window = HeadlessWindow::new(120, 80);
    let mut handler = overlaid::<Shield>(Box::new(LocalApp(Listening(Rc::clone(&heard)))));
    handler.tree = Some(handler.app.mount());

    handler.on_event(press_at(10.0, 10.0), &window);
    assert_eq!(heard.get(), 0, "the overlay took the press");
    handler.on_event(
        Event::PointerMoved {
            x: 10.0,
            y: 10.0,
            source: platform_core::PointerSource::Mouse,
        },
        &window,
    );
    assert_eq!(handler.dev.seen, 2, "the overlay sees every event first");

    let mut bare = overlaid::<()>(Box::new(LocalApp(Listening(Rc::clone(&heard)))));
    bare.tree = Some(bare.app.mount());
    bare.on_event(press_at(10.0, 10.0), &window);
    assert_eq!(heard.get(), 1, "with no overlay the press is the app's");
}

/// An overlay that wants idle frames, and optionally the accessibility tree.
#[derive(Default)]
struct Watching {
    frames: u32,
    reading: bool,
    heard: Vec<String>,
}

impl DevOverlay for Watching {
    fn on_frame<'a>(
        &mut self,
        base: &'a [renderer_core::DrawCommand],
        _window_w: f32,
        _window_h: f32,
        _tree_dirty: bool,
    ) -> std::borrow::Cow<'a, [renderer_core::DrawCommand]> {
        self.frames += 1;
        std::borrow::Cow::Borrowed(base)
    }

    fn needs_frame(&self) -> bool {
        true
    }

    fn wants_access(&self) -> bool {
        self.reading
    }

    fn on_access(&mut self, nodes: &[platform_core::AccessNode]) {
        self.heard = nodes.iter().map(|n| n.name.clone()).collect();
    }
}

/// An FPS readout has to fall to zero once the app stops drawing, which it can only do if the overlay is still handed frames.
#[test]
fn an_overlay_that_needs_frames_keeps_them_coming_while_the_app_is_idle() {
    let window = HeadlessWindow::new(120, 80);
    let mut handler = overlaid::<Watching>(Box::new(LocalApp(Unchanging)));
    handler.new_events();
    assert!(handler.on_resume(&window));
    handler.about_to_wait();
    run_a_pass(&mut handler, &window);
    let settled = handler.dev.frames;

    assert_eq!(
        handler.about_to_wait(),
        Some(HW_KEEPALIVE_INTERVAL),
        "a clean tree still wakes the loop for the overlay"
    );
    handler.pacer.last_submit = web_time::Instant::now() - HW_KEEPALIVE_INTERVAL;
    run_a_pass(&mut handler, &window);
    assert_eq!(
        handler.dev.frames,
        settled + 1,
        "and the wake composes a frame"
    );
}

/// An overlay whose picture changes while the app beneath it stays still.
#[derive(Default)]
struct Repainting {
    dirty: bool,
    frames: u32,
}

impl DevOverlay for Repainting {
    fn on_frame<'a>(
        &mut self,
        base: &'a [renderer_core::DrawCommand],
        _window_w: f32,
        _window_h: f32,
        _tree_dirty: bool,
    ) -> std::borrow::Cow<'a, [renderer_core::DrawCommand]> {
        self.frames += 1;
        self.dirty = false;
        std::borrow::Cow::Borrowed(base)
    }

    fn is_dirty(&self) -> bool {
        self.dirty
    }
}

/// A renderer re-presents a frame whose generation it already drew, so an overlay that changed over a still app has to move the number, and its change must not wait for a keepalive that a rasterising run never takes.
#[test]
fn an_overlay_that_changed_is_drawn_at_once_under_a_new_generation() {
    let window = HeadlessWindow::new(120, 80);
    let mut handler = overlaid::<Repainting>(Box::new(LocalApp(Unchanging)));
    handler.new_events();
    assert!(handler.on_resume(&window));
    handler.about_to_wait();
    run_a_pass(&mut handler, &window);
    let settled = handler.dev.frames;
    let drawn = handler.generation.last;

    run_a_pass(&mut handler, &window);
    assert_eq!(
        handler.dev.frames, settled,
        "precondition: a clean overlay over a clean tree is handed no frame"
    );

    handler.dev.dirty = true;
    run_a_pass(&mut handler, &window);
    assert_eq!(
        handler.dev.frames,
        settled + 1,
        "the changed overlay is composed in the next pass"
    );
    assert!(
        handler.generation.last > drawn,
        "under a generation the renderer has not drawn"
    );
}

/// Counts the accessibility snapshots the runner asks the app's runtime for.
struct Counted<A: App> {
    app: LocalApp<A>,
    snapshots: Rc<Cell<u32>>,
}

impl<A: App> AppRuntime for Counted<A> {
    fn mount(&mut self) -> Box<dyn crate::tree::UiTree> {
        self.app.mount()
    }

    fn clear_color(&self) -> Option<renderer_core::Color> {
        self.app.clear_color()
    }

    fn window_config(&self) -> Option<platform_core::WindowConfig> {
        self.app.window_config()
    }

    fn on_frame(&mut self, ctx: &mut platform_core::AppCtx) {
        self.app.on_frame(ctx)
    }

    fn access_audit(
        &self,
        frame: &[renderer_core::DrawCommand],
    ) -> Option<platform_core::AccessAudit> {
        self.snapshots.set(self.snapshots.get() + 1);
        self.app.access_audit(frame)
    }
}

fn counted(snapshots: &Rc<Cell<u32>>) -> Box<dyn AppRuntime> {
    Box::new(Counted {
        app: LocalApp(Labelled),
        snapshots: Rc::clone(snapshots),
    })
}

#[test]
fn an_overlay_that_asks_reads_the_window_as_a_screen_reader_would() {
    crate::install_default_text_metrics();
    let snapshots = Rc::new(Cell::new(0));
    let window = HeadlessWindow::new(200, 40);
    let mut handler = overlaid::<Watching>(counted(&snapshots));
    handler.dev.reading = true;
    handler.new_events();
    assert!(handler.on_resume(&window));
    run_a_pass(&mut handler, &window);

    assert_eq!(snapshots.get(), 1);
    assert!(
        handler.dev.heard.iter().any(|name| name == "Configured"),
        "the overlay hears the text the app drew: {:?}",
        handler.dev.heard
    );
}

/// Building the snapshot walks the focus registry and every drawn text, which a frame with no reader for it must not pay for.
#[test]
fn no_snapshot_is_built_unless_the_overlay_asks() {
    crate::install_default_text_metrics();
    let window = HeadlessWindow::new(200, 40);

    let snapshots = Rc::new(Cell::new(0));
    let mut bare = overlaid::<()>(counted(&snapshots));
    bare.new_events();
    assert!(bare.on_resume(&window));
    run_a_pass(&mut bare, &window);
    assert_eq!(snapshots.get(), 0, "`()` costs nothing");

    let mut quiet = overlaid::<Watching>(counted(&snapshots));
    quiet.new_events();
    assert!(quiet.on_resume(&window));
    run_a_pass(&mut quiet, &window);
    assert!(quiet.dev.frames > 0, "precondition: a frame was composed");
    assert_eq!(
        snapshots.get(),
        0,
        "an overlay that does not ask is not handed one"
    );
}

/// An overlay that reads the window and draws a word of its own over it.
#[derive(Default)]
struct Inspecting {
    heard: Vec<String>,
    findings: Option<usize>,
}

impl DevOverlay for Inspecting {
    fn on_frame<'a>(
        &mut self,
        base: &'a [renderer_core::DrawCommand],
        _window_w: f32,
        _window_h: f32,
        _tree_dirty: bool,
    ) -> std::borrow::Cow<'a, [renderer_core::DrawCommand]> {
        let mut drawn = base.to_vec();
        drawn.push(renderer_core::DrawCommand::Text {
            text: "Overlay".into(),
            spans: None,
            rect: geometry_core::Rect::new(0.0, 20.0, 60.0, 16.0),
            style: Arc::new(renderer_core::TextStyle::new(
                12.0,
                renderer_core::Color::BLACK,
            )),
        });
        std::borrow::Cow::Owned(drawn)
    }

    fn wants_access(&self) -> bool {
        true
    }

    fn on_access(&mut self, nodes: &[platform_core::AccessNode]) {
        self.heard = nodes.iter().map(|n| n.name.clone()).collect();
    }

    fn on_violations(&mut self, violations: &[platform_core::AccessViolation]) {
        self.findings = Some(violations.len());
    }
}

#[test]
fn an_overlay_reads_the_apps_frame_with_what_the_checks_found_and_not_its_own() {
    crate::install_default_text_metrics();
    let window = HeadlessWindow::new(200, 40);
    let mut handler = overlaid::<Inspecting>(Box::new(LocalApp(Labelled)));
    handler.new_events();
    assert!(handler.on_resume(&window));
    run_a_pass(&mut handler, &window);
    run_a_pass(&mut handler, &window);

    assert!(
        handler.dev.heard.iter().any(|name| name == "Configured"),
        "{:?}",
        handler.dev.heard
    );
    assert!(
        !handler.dev.heard.iter().any(|name| name == "Overlay"),
        "the overlay inspects the app, not itself: {:?}",
        handler.dev.heard
    );
    assert!(
        handler.dev.findings.is_some(),
        "and is told what the checks found in the same frame"
    );
}

thread_local! {
    static MOVES_ASKED: Cell<u32> = const { Cell::new(0) };
    static MOVES_ENDED: Cell<u32> = const { Cell::new(0) };
    static SHOWN_MAXIMIZED: Cell<Option<bool>> = const { Cell::new(None) };
}

/// A 120x80 window drawing its own title bar: no decorations from the window system, and a 120x20 band along the top that moves the window, counting each move it asks for and each stroke that ends.
struct TitleBar;

impl App for TitleBar {
    fn root(&self) -> Box<dyn ui_tree::Component> {
        ui_core::reset_layout_runtime();
        crate::window::set_decorations(false);
        reactive_core::effect(|| {
            SHOWN_MAXIMIZED.with(|shown| shown.set(Some(crate::window::is_maximized())))
        });
        let band = ui_core::StyledContainer::new(
            layout_core::LayoutStyle::new().width(120.0).height(20.0),
            |_| renderer_core::RectStyle::default(),
            vec![],
        )
        .expect("a band builds")
        .on_drag(|_, _| {
            MOVES_ASKED.with(|asked| asked.set(asked.get() + 1));
            crate::window::drag();
        })
        .on_drag_end(|_, _| MOVES_ENDED.with(|ended| ended.set(ended.get() + 1)));
        let card = ui_core::StyledContainer::new(
            layout_core::LayoutStyle::new()
                .flex_column()
                .width(120.0)
                .height(80.0),
            |_| renderer_core::RectStyle::default(),
            vec![ui_core::box_item(band)],
        )
        .expect("a card builds");
        Box::new(ui_core::WindowRoot::new(ui_core::box_item(card)))
    }
}

fn title_bar_handler() -> AppHandler<HeadlessWindow, ()> {
    build_app_handler::<HeadlessWindow, ()>(
        Box::new(LocalApp(TitleBar)),
        Arc::new(services_core::NoPaths),
        crate::runner::font_config::FontSetup::default(),
        RendererBackend::Software,
        UserPrefs::default(),
        "title-bar-test".to_string(),
        SurfaceRenderer::builtin(),
    )
}

#[test]
fn a_tree_that_draws_its_own_title_bar_is_never_shown_with_the_systems() {
    let window = HeadlessWindow::new(120, 80);
    let mut handler = title_bar_handler();
    handler.new_events();
    assert!(handler.on_resume(&window));
    assert!(
        !window.is_decorated(),
        "the decorations are gone when the resume hands the window back to be shown"
    );
}

/// The window system keeps the release of a stroke it took over to move the window, so the runner ends the stroke itself: left running, the next move over the window would ask for another move with no button held.
#[test]
fn a_stroke_the_window_system_takes_over_ends_where_it_was_handed_over() {
    use platform_core::{PointerButton, PointerSource};

    MOVES_ASKED.with(|asked| asked.set(0));
    MOVES_ENDED.with(|ended| ended.set(0));
    let window = HeadlessWindow::new(120, 80);
    let mut handler = title_bar_handler();
    handler.new_events();
    assert!(handler.on_resume(&window));
    run_a_pass(&mut handler, &window);

    handler.on_event(
        Event::PointerPressed {
            x: 60.0,
            y: 10.0,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        },
        &window,
    );
    assert_eq!(MOVES_ASKED.with(Cell::get), 1);
    assert_eq!(
        MOVES_ENDED.with(Cell::get),
        1,
        "the stroke ended with the handover"
    );
    assert!(
        !ui_core::pointer_buttons().any(),
        "and nothing is held any more"
    );

    handler.on_event(
        Event::PointerMoved {
            x: 70.0,
            y: 10.0,
            source: PointerSource::Mouse,
        },
        &window,
    );
    assert_eq!(
        MOVES_ASKED.with(Cell::get),
        1,
        "a later move asks for nothing"
    );
}

#[test]
fn the_app_hears_whether_its_window_is_maximized_as_the_window_resizes() {
    let window = HeadlessWindow::new(120, 80);
    let mut handler = title_bar_handler();
    handler.new_events();
    assert!(handler.on_resume(&window));
    handler.about_to_wait();
    assert_eq!(SHOWN_MAXIMIZED.with(Cell::get), Some(false));

    window.set_maximized(true);
    window.resize(400, 300);
    handler.new_events();
    handler.on_event(
        Event::WindowResized {
            width: 400,
            height: 300,
        },
        &window,
    );
    handler.about_to_wait();
    assert_eq!(SHOWN_MAXIMIZED.with(Cell::get), Some(true));
}

#[cfg(all(feature = "dev", not(target_arch = "wasm32")))]
thread_local! {
    static PRESSES_HEARD: Cell<u32> = const { Cell::new(0) };
}

/// A tree whose press handler panics, as one that writes a signal its own effect still borrows does.
#[cfg(all(feature = "dev", not(target_arch = "wasm32")))]
struct BorrowsTwice;

#[cfg(all(feature = "dev", not(target_arch = "wasm32")))]
impl ui_tree::Component for BorrowsTwice {
    fn view(&self) -> ui_tree::RenderNode {
        ui_tree::RenderNode::group([])
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        if matches!(event, Event::PointerPressed { .. }) {
            PRESSES_HEARD.with(|heard| heard.set(heard.get() + 1));
            panic!("RUNTIME is already borrowed");
        }
        EventResult::Ignored
    }
}

#[cfg(all(feature = "dev", not(target_arch = "wasm32")))]
struct Panicking;

#[cfg(all(feature = "dev", not(target_arch = "wasm32")))]
impl App for Panicking {
    fn root(&self) -> Box<dyn ui_tree::Component> {
        Box::new(BorrowsTwice)
    }
}

/// A tree that hears presses and does nothing else with them.
#[cfg(all(feature = "dev", not(target_arch = "wasm32")))]
struct Counting;

#[cfg(all(feature = "dev", not(target_arch = "wasm32")))]
impl ui_tree::Component for Counting {
    fn view(&self) -> ui_tree::RenderNode {
        ui_tree::RenderNode::group([])
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        if matches!(event, Event::PointerPressed { .. }) {
            PRESSES_HEARD.with(|heard| heard.set(heard.get() + 1));
        }
        EventResult::Ignored
    }
}

#[cfg(all(feature = "dev", not(target_arch = "wasm32")))]
struct Fixed;

#[cfg(all(feature = "dev", not(target_arch = "wasm32")))]
impl App for Fixed {
    fn root(&self) -> Box<dyn ui_tree::Component> {
        Box::new(Counting)
    }
}

#[cfg(all(feature = "dev", not(target_arch = "wasm32")))]
fn create_panicking() -> crate::hot::GuestResult<Box<dyn App>> {
    crate::hot::guest::create_app(|| Box::new(Panicking))
}

#[cfg(all(feature = "dev", not(target_arch = "wasm32")))]
fn create_fixed() -> crate::hot::GuestResult<Box<dyn App>> {
    crate::hot::guest::create_app(|| Box::new(Fixed))
}

#[cfg(all(feature = "dev", not(target_arch = "wasm32")))]
fn create_nothing() -> crate::hot::GuestResult<Box<dyn App>> {
    crate::hot::guest::create_app(|| panic!("the app's setup could not find its assets"))
}

/// A build served by this process's own exports, through the same table, the same catch and the same bookkeeping a dylib's go through. What stays untested here is the second copy of std a real dylib carries.
#[cfg(all(feature = "dev", not(target_arch = "wasm32")))]
fn hot_build(create: crate::hot::CreateApp) -> Box<dyn AppRuntime> {
    fn keep_services(_: crate::hot::HostServices) -> crate::hot::GuestResult<()> {
        Ok(())
    }
    let exports = crate::hot::GuestExports {
        install_services: keep_services,
        ..crate::hot::GuestExports::in_process()
    };
    Box::new(crate::hot::HotApp::start(exports, create, None))
}

/// What the overlay was told about builds and panics.
#[cfg(all(feature = "dev", not(target_arch = "wasm32")))]
#[derive(Default)]
struct Reporting {
    build_error: Option<String>,
    app_panic: Option<String>,
}

#[cfg(all(feature = "dev", not(target_arch = "wasm32")))]
impl DevOverlay for Reporting {
    fn on_frame<'a>(
        &mut self,
        base: &'a [renderer_core::DrawCommand],
        _window_w: f32,
        _window_h: f32,
        _tree_dirty: bool,
    ) -> std::borrow::Cow<'a, [renderer_core::DrawCommand]> {
        std::borrow::Cow::Borrowed(base)
    }

    fn set_build_error(&mut self, error: Option<String>) {
        self.build_error = error;
    }

    fn set_app_panic(&mut self, panic: Option<String>) {
        self.app_panic = panic;
    }
}

/// A panic in the app's event handler used to unwind out of the dylib into the host, whose copy of std can only abort on it: the window closed and the session with it. Now the window stays, says what panicked and where, runs nothing more of that build, and the next build takes over.
#[cfg(all(feature = "dev", not(target_arch = "wasm32")))]
#[test]
fn a_panic_in_the_app_is_shown_and_the_next_build_takes_over() {
    let window = HeadlessWindow::new(120, 80);
    let mut handler = overlaid::<Reporting>(hot_build(create_panicking));
    handler.new_events();
    assert!(handler.on_resume(&window));
    handler.about_to_wait();
    assert_eq!(
        handler.dev.app_panic, None,
        "precondition: a build that runs"
    );

    handler.new_events();
    handler.on_event(press_at(10.0, 10.0), &window);
    assert_eq!(PRESSES_HEARD.with(Cell::get), 1);
    handler.about_to_wait();
    let report = handler
        .dev
        .app_panic
        .clone()
        .expect("the overlay is told the app panicked");
    assert!(report.contains("RUNTIME is already borrowed"), "{report}");
    assert!(
        report.contains(&format!("panicked at {}:", file!())),
        "the report says where: {report}"
    );
    assert!(handler.tree.is_none(), "the tree that panicked is let go");

    handler.new_events();
    handler.on_event(press_at(10.0, 10.0), &window);
    handler.on_redraw(&window);
    handler.about_to_wait();
    assert_eq!(
        PRESSES_HEARD.with(Cell::get),
        1,
        "nothing more of the build that panicked runs"
    );

    handler.dev.build_error = Some("error: expected `;`".to_owned());
    handler.new_events();
    handler.take_build(hot_build(create_fixed), &window);
    handler.about_to_wait();
    assert_eq!(handler.dev.app_panic, None, "a build that loads clears it");
    assert_eq!(handler.dev.build_error, None);
    handler.new_events();
    handler.on_event(press_at(10.0, 10.0), &window);
    assert_eq!(
        PRESSES_HEARD.with(Cell::get),
        2,
        "the new build hears the press"
    );
}

/// A build whose setup panics leaves no app to run, but the window it would have drawn in is the one the next build draws in.
#[cfg(all(feature = "dev", not(target_arch = "wasm32")))]
#[test]
fn a_build_that_panics_as_it_starts_keeps_the_window_for_the_next() {
    let window = HeadlessWindow::new(120, 80);
    let mut handler = overlaid::<Reporting>(hot_build(create_nothing));
    assert!(handler.app.window_config().is_none());
    handler.new_events();
    assert!(handler.on_resume(&window));
    handler.on_redraw(&window);
    handler.about_to_wait();
    let report = handler
        .dev
        .app_panic
        .clone()
        .expect("the overlay is told the app panicked");
    assert!(
        report.contains("the app's setup could not find its assets"),
        "{report}"
    );

    handler.new_events();
    handler.take_build(hot_build(create_fixed), &window);
    handler.about_to_wait();
    assert_eq!(handler.dev.app_panic, None);
    assert!(handler.tree.is_some(), "the next build mounts its tree");
}

/// A window with one field, focused as it is built.
struct OneField;

impl App for OneField {
    fn root(&self) -> Box<dyn ui_tree::Component> {
        ui_core::reset_layout_runtime();
        let field = ui_core::Input::new(
            reactive_core::signal(String::from("ab")),
            layout_core::LayoutStyle::new().width(100.0).height(20.0),
            || renderer_core::TextStyle::new(14.0, renderer_core::Color::BLACK),
        )
        .expect("a field builds")
        .autofocus();
        Box::new(ui_core::WindowRoot::new(ui_core::box_item(field)))
    }
}

/// **The window hears about the field once a frame has drawn it.** An input method composes into a window only while the window has been told a field takes text, and its candidates open where that field's caret was drawn; a field letting the keyboard go takes the input method away with it.
#[test]
fn the_window_is_told_where_the_focused_field_has_its_caret() {
    let window = HeadlessWindow::new(120, 80);
    let mut handler = build_app_handler::<HeadlessWindow, ()>(
        Box::new(LocalApp(OneField)),
        Arc::new(services_core::NoPaths),
        crate::runner::font_config::FontSetup::default(),
        RendererBackend::Software,
        UserPrefs::default(),
        "text-input-test".to_string(),
        SurfaceRenderer::builtin(),
    );
    handler.new_events();
    assert!(handler.on_resume(&window));
    run_a_pass(&mut handler, &window);
    let input = window.text_input().expect("the focused field was reported");
    assert!(input.caret.x > 0.0 && input.caret.height > 0.0, "{input:?}");
    assert!(!input.secret);

    ui_core::focus::clear();
    run_a_pass(&mut handler, &window);
    assert_eq!(window.text_input(), None, "no field takes text any more");
}

thread_local! {
    static UNLOAD_JOURNAL: std::cell::RefCell<Vec<&'static str>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn note(event: &'static str) {
    UNLOAD_JOURNAL.with(|journal| journal.borrow_mut().push(event));
}

fn unload_journal() -> Vec<&'static str> {
    UNLOAD_JOURNAL.with(|journal| journal.borrow().clone())
}

/// An app that notes when it is dropped: for a hot-reloaded one, the moment its library is unmapped.
#[cfg(all(feature = "dev", not(target_arch = "wasm32")))]
struct Unloading(LocalApp<Unchanging>);

#[cfg(all(feature = "dev", not(target_arch = "wasm32")))]
impl AppRuntime for Unloading {
    fn mount(&mut self) -> Box<dyn crate::tree::UiTree> {
        self.0.mount()
    }

    fn clear_color(&self) -> Option<renderer_core::Color> {
        self.0.clear_color()
    }

    fn window_config(&self) -> Option<platform_core::WindowConfig> {
        self.0.window_config()
    }

    fn on_frame(&mut self, ctx: &mut platform_core::AppCtx) {
        self.0.on_frame(ctx)
    }
}

#[cfg(all(feature = "dev", not(target_arch = "wasm32")))]
impl Drop for Unloading {
    fn drop(&mut self) {
        note("app unloaded");
    }
}

#[derive(Default)]
struct Forgetting;

impl DevOverlay for Forgetting {
    fn on_frame<'a>(
        &mut self,
        base: &'a [renderer_core::DrawCommand],
        _window_w: f32,
        _window_h: f32,
        _tree_dirty: bool,
    ) -> std::borrow::Cow<'a, [renderer_core::DrawCommand]> {
        std::borrow::Cow::Borrowed(base)
    }

    fn forget_frames(&mut self) {
        note("overlay forgot");
    }
}

struct ForgettingHost(crate::runner::host::BuiltinHost<HeadlessWindow>);

impl RendererHost<HeadlessWindow> for ForgettingHost {
    fn start(&mut self, window: &HeadlessWindow, req: &RendererRequest<'_>) -> RendererStart {
        self.0.start(window, req)
    }

    fn channels(&self) -> Option<&RenderChannels> {
        self.0.channels()
    }

    fn forget_frames(&mut self) {
        self.0.forget_frames();
        note("renderer forgot");
    }

    fn suspend(&mut self) {
        self.0.suspend();
    }

    fn retire(&mut self) {
        self.0.retire();
    }

    fn build_offscreen(
        &mut self,
        window: &HeadlessWindow,
        req: &RendererRequest<'_>,
    ) -> Option<Box<dyn RenderBackend>> {
        self.0.build_offscreen(window, req)
    }
}

#[cfg(all(feature = "dev", not(target_arch = "wasm32")))]
fn unloading_handler(window: &HeadlessWindow) -> AppHandler<HeadlessWindow, Forgetting> {
    let mut handler = build_app_handler::<HeadlessWindow, Forgetting>(
        Box::new(Unloading(LocalApp(Unchanging))),
        Arc::new(services_core::NoPaths),
        crate::runner::font_config::FontSetup::default(),
        RendererBackend::Software,
        UserPrefs::default(),
        "unload-test".to_string(),
        SurfaceRenderer::builtin(),
    );
    handler.renderer_host = Box::new(ForgettingHost(crate::runner::host::BuiltinHost::new()));
    handler.new_events();
    assert!(handler.on_resume(window));
    handler.on_redraw(window);
    handler
}

#[cfg(all(feature = "dev", not(target_arch = "wasm32")))]
fn forgot_before_unloading(journal: &[&str]) -> bool {
    let at = |event: &str| journal.iter().position(|noted| *noted == event);
    let unloaded = at("app unloaded").expect("the app is unloaded");
    at("renderer forgot").is_some_and(|forgot| forgot < unloaded)
        && at("overlay forgot").is_some_and(|forgot| forgot < unloaded)
}

/// A command can point into the image of the library that built it, so a renderer still holding a frame when that library is unmapped crashes the process as it lets go.
#[cfg(all(feature = "dev", not(target_arch = "wasm32")))]
#[test]
fn a_reload_forgets_every_frame_before_the_outgoing_app_is_unloaded() {
    let window = HeadlessWindow::new(40, 40);
    let mut handler = unloading_handler(&window);
    handler.swap_app(Box::new(LocalApp(Unchanging)), &window);
    assert!(
        forgot_before_unloading(&unload_journal()),
        "{:?}",
        unload_journal()
    );
}

#[cfg(all(feature = "dev", not(target_arch = "wasm32")))]
#[test]
fn closing_forgets_every_frame_before_the_app_is_unloaded() {
    let window = HeadlessWindow::new(40, 40);
    drop(unloading_handler(&window));
    assert!(
        forgot_before_unloading(&unload_journal()),
        "{:?}",
        unload_journal()
    );
}
