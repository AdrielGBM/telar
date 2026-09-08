use super::*;
use crate::app::App;
use crate::app_runtime::LocalApp;
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
