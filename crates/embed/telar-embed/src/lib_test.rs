use super::*;
use crate::guest::GuestInstance;
use layout_core::LayoutStyle;
#[cfg(all(feature = "host", not(target_arch = "wasm32")))]
use platform_core::guest::Lifecycle;
use platform_core::guest::{CWaker, HostServices};
use platform_core::{Key, KeyboardConvention, ModifiersState};
use renderer_core::RectStyle;

struct Stub {
    node: Option<NodeId>,
    seen: usize,
}

impl Stub {
    fn new() -> Self {
        Self {
            node: None,
            seen: 0,
        }
    }
}

impl EmbeddedApp for Stub {
    fn build(&mut self) {
        let (node, _) =
            ui_core::new_leaf(LayoutStyle::new().width(40.0).height(20.0)).expect("leaf");
        self.node = Some(node);
    }
    fn layout_root(&self) -> NodeId {
        self.node.expect("build ran first")
    }
    fn view(&self) -> RenderNode {
        RenderNode::rect(Rect::new(0.0, 0.0, 40.0, 20.0), RectStyle::default())
    }
    fn on_event(&mut self, _event: &Event) -> EventResult {
        self.seen += 1;
        EventResult::Ignored
    }
    fn title(&self) -> String {
        "stub".into()
    }
    fn id(&self) -> String {
        "stub".into()
    }
}

fn shift() -> ModifiersState {
    ModifiersState {
        is_shift: true,
        ..ModifiersState::default()
    }
}

// Expanded here too, so a change that breaks the macro fails this crate's own tests rather than only the fixtures `telar`'s ignored `guest_contract` suite builds.
crate::embed!(|_args: &[String]| -> Box<dyn EmbeddedApp> { Box::new(Stub::new()) });

/// Builds an instance through the exported table, and hands it back with the outcome.
fn create_through_table() -> (CallOutcome, Handle<GuestInstance>) {
    let args = wire_core::encode(&Vec::<String>::new());
    let mut instance = Handle::NULL;
    let outcome = unsafe { (TELAR_EMBED_ENTRIES.create)(LentBytes::lend(&args), &mut instance) };
    (outcome, instance)
}

#[test]
fn the_export_macro_builds_a_table_that_answers_in_bytes() {
    let (outcome, instance) = create_through_table();
    assert_eq!(outcome.status(), CallOutcome::RETURNED);
    let mut id = LentBytes::EMPTY;
    let outcome = unsafe { (TELAR_EMBED_ENTRIES.id)(instance, &mut id) };
    assert_eq!(outcome.status(), CallOutcome::RETURNED);
    assert_eq!(
        unsafe { id.decode::<String>() }.expect("the id is a string"),
        "stub"
    );
    let dark = SystemPreferences {
        color_scheme: Some(platform_core::ColorScheme::Dark),
        reduced_motion: Some(true),
        ..SystemPreferences::default()
    };
    let spelled = wire_core::encode(&dark);
    let outcome = unsafe {
        (TELAR_EMBED_ENTRIES.set_system_preferences)(instance, LentBytes::lend(&spelled))
    };
    assert_eq!(outcome.status(), CallOutcome::RETURNED);
    assert_eq!(
        preferences_core::system_preferences(),
        dark,
        "the whole snapshot crosses, not only the scheme"
    );
    preferences_core::set_system_preferences(SystemPreferences::default());
    let outcome = unsafe { (TELAR_EMBED_ENTRIES.release)(instance) };
    assert_eq!(outcome.status(), CallOutcome::RETURNED);
}

// These deliberately never observe on the caller's behalf, so they fail the moment either observe call leaves `EmbedInstance::on_event`.
#[test]
fn a_guest_records_the_modifiers_it_is_handed() {
    let mut inst = EmbedInstance::new(Box::new(Stub::new()));
    inst.canvas
        .with_entered(|| assert_eq!(ui_core::modifiers(), ModifiersState::default()));

    inst.on_event(&Event::ModifiersChanged { modifiers: shift() });

    inst.canvas.with_entered(|| {
        assert!(
            ui_core::modifiers().is_shift,
            "a shift-drag inside a guest is indistinguishable from a plain one without this"
        );
    });
}

#[test]
fn an_overlay_event_reaches_the_registry_too() {
    let inst = EmbedInstance::new(Box::new(Stub::new()));
    inst.dispatch_overlays(&Event::ModifiersChanged { modifiers: shift() });

    inst.canvas.with_entered(|| {
        assert!(
            ui_core::modifiers().is_shift,
            "an overlay event reaches the shared registry"
        );
    })
}

#[test]
fn a_press_answers_for_one_frame_and_end_frame_closes_it() {
    let mut inst = EmbedInstance::new(Box::new(Stub::new()));
    inst.on_event(&Event::KeyPressed {
        key: Key::Char('c'),
        modifiers: ModifiersState::default(),
        unmodified: None,
    });

    inst.canvas.with_entered(|| {
        assert!(
            ui_core::key_pressed(&Key::Char('c')),
            "the press answers for the frame it arrived in"
        );
    });
    inst.end_frame();
    inst.canvas.with_entered(|| {
        assert!(
            !ui_core::key_pressed(&Key::Char('c')),
            "without end_frame the press answers forever, not for its frame"
        );
        assert!(
            ui_core::key_held(&Key::Char('c')),
            "held is not what end_frame clears"
        );
    })
}

#[test]
fn a_guest_paints_and_its_generation_is_stable_between_frames() {
    let mut inst = EmbedInstance::new(Box::new(Stub::new()));
    inst.relayout(40.0, 20.0);
    assert!(
        !inst.paint().is_empty(),
        "a guest that paints emits commands"
    );
    assert_eq!(inst.generation(), inst.generation());
}

#[test]
fn the_driver_forwards_metadata_from_the_embedded_app() {
    let inst = EmbedInstance::new(Box::new(Stub::new()));
    assert_eq!(inst.title(), "stub");
    assert_eq!(inst.id(), "stub");
    assert_eq!(inst.clear_color(), None);
}

/// A foreign surface that reports nothing, for a boundary to mark a frame with.
struct Silent;

impl ForeignSurface for Silent {
    fn access_snapshot(&self) -> Vec<platform_core::AccessNode> {
        Vec::new()
    }
    fn perform_access_action(&self, _id: u64, _activate: bool) {}
    fn text_input(&self) -> Option<platform_core::TextInput> {
        None
    }
    fn arrive(&self, _backwards: bool) -> bool {
        false
    }
    fn tab(&self, _key: &Event) -> bool {
        false
    }
    fn blur(&self) {}
    fn holds_focus(&self) -> bool {
        false
    }
    fn text_entry_focused(&self) -> bool {
        false
    }
    fn keeps(&self, _keys: platform_core::ConsumedKeys) -> bool {
        false
    }
}

#[test]
fn composite_translates_into_the_sub_rect_clips_to_it_and_marks_the_guests_frame() {
    let rect = Rect::new(10.0, 20.0, 100.0, 50.0);
    let silent: Rc<dyn ForeignSurface> = Rc::new(Silent);
    let boundary = ForeignBoundary::new(&silent);
    let node = composite(
        rect,
        &boundary,
        vec![DrawCommand::Rect {
            rect: Rect::new(0.0, 0.0, 5.0, 5.0),
            style: std::sync::Arc::new(RectStyle::default()),
        }],
    );
    let RenderNode::Clip {
        rect: clip,
        children,
        ..
    } = node
    else {
        panic!("expected the guest's frame to be wrapped in a clip");
    };
    assert_eq!(clip, rect, "clipped to the host's sub-rect");
    let drawn = ui_tree::ComponentList::new(Drawn(RefCell::new(Some(RenderNode::group(children)))))
        .commands()
        .clone();
    assert!(
        drawn.contains(&DrawCommand::PushSurface {
            surface: boundary.key()
        }),
        "the guest's frame is marked as its: {drawn:?}"
    );
}

/// A host's tree that draws one node once.
struct Drawn(RefCell<Option<RenderNode>>);

impl Component for Drawn {
    fn view(&self) -> RenderNode {
        self.0.borrow_mut().take().unwrap_or(RenderNode::Empty)
    }
}

/// A guest's frame crosses as bytes its own encoder lent, and a host decodes it into commands of its own that draw what the guest's canvas holds.
#[test]
fn a_frame_crosses_as_bytes_the_host_decodes_into_its_own_commands() {
    let (_, instance) = create_through_table();
    unsafe { (TELAR_EMBED_ENTRIES.relayout)(instance, 40.0, 20.0) };
    let mut frame = LentBytes::EMPTY;
    let outcome = unsafe { (TELAR_EMBED_ENTRIES.paint)(instance, &mut frame) };
    assert_eq!(outcome.status(), CallOutcome::RETURNED);
    let mut commands = Vec::new();
    renderer_core::wire::FrameDecoder::new()
        .decode(unsafe { frame.as_slice() }, &mut commands, |_| None)
        .expect("the frame decodes");
    assert_eq!(commands.len(), 1, "{commands:?}");
    assert!(matches!(commands[0], DrawCommand::Rect { .. }));
    unsafe { (TELAR_EMBED_ENTRIES.release)(instance) };
}

const DELAY: Duration = Duration::from_millis(500);

/// A guest that schedules a timer as it builds and counts the moves its content hears.
struct Waiting {
    node: Option<NodeId>,
    fired: Rc<std::cell::Cell<bool>>,
    moves: Rc<std::cell::Cell<u32>>,
    timer: Option<reactive_core::Timer>,
}

impl Waiting {
    fn new(fired: &Rc<std::cell::Cell<bool>>, moves: &Rc<std::cell::Cell<u32>>) -> Self {
        Self {
            node: None,
            fired: Rc::clone(fired),
            moves: Rc::clone(moves),
            timer: None,
        }
    }
}

impl EmbeddedApp for Waiting {
    fn build(&mut self) {
        let (node, _) =
            ui_core::new_leaf(LayoutStyle::new().width(40.0).height(20.0)).expect("leaf");
        self.node = Some(node);
        let fired = Rc::clone(&self.fired);
        self.timer = Some(reactive_core::run_after(DELAY, move || fired.set(true)));
    }
    fn layout_root(&self) -> NodeId {
        self.node.expect("build ran first")
    }
    fn view(&self) -> RenderNode {
        RenderNode::rect(Rect::new(0.0, 0.0, 40.0, 20.0), RectStyle::default())
    }
    fn on_event(&mut self, event: &Event) -> EventResult {
        if matches!(event, Event::PointerMoved { .. }) {
            self.moves.set(self.moves.get() + 1);
        }
        EventResult::Ignored
    }
    fn title(&self) -> String {
        "waiting".into()
    }
    fn id(&self) -> String {
        "waiting".into()
    }
}

fn waiting() -> (
    EmbedInstance,
    Rc<std::cell::Cell<bool>>,
    Rc<std::cell::Cell<u32>>,
) {
    let fired = Rc::new(std::cell::Cell::new(false));
    let moves = Rc::new(std::cell::Cell::new(0));
    let inst = EmbedInstance::new(Box::new(Waiting::new(&fired, &moves)));
    (inst, fired, moves)
}

fn run_frame(inst: &mut EmbedInstance, waker: Option<&platform_core::RedrawWaker>) {
    let mut redraw_requested = false;
    let mut ctx = AppCtx::new(&mut redraw_requested, waker, None, None);
    inst.on_frame(&mut ctx);
}

#[test]
fn a_guest_timer_is_reported_until_its_frame_fires_it() {
    let (mut inst, fired, _) = waiting();
    let due = inst
        .next_timer_due()
        .expect("the guest reports its deadline");
    let wait = inst.until_next_timer().expect("and how long until it");
    assert!(wait <= DELAY && wait > DELAY / 2, "{wait:?}");

    run_frame(&mut inst, None);
    assert!(!fired.get(), "not before it is due");
    assert_eq!(inst.next_timer_due(), Some(due), "the deadline holds still");

    reactive_core::advance_timer_clock(DELAY);
    run_frame(&mut inst, None);
    assert!(fired.get(), "the guest's frame runs what came due");
    assert_eq!(inst.until_next_timer(), None);
}

#[test]
fn a_timer_the_guest_schedules_wakes_the_host_loop() {
    let (mut inst, _, _) = waiting();
    let this_thread = std::thread::current().id();
    let wakes = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    let counting = std::sync::Arc::clone(&wakes);
    // The waker slot is process-wide; counting only this thread's wakes keeps the other tests' timers out of the count.
    let waker = platform_core::RedrawWaker::new(move || {
        if std::thread::current().id() == this_thread {
            counting.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    });
    let outcome = unsafe { (TELAR_EMBED_ENTRIES.install_task_waker)(CWaker::new(waker.clone())) };
    assert_eq!(outcome.status(), CallOutcome::RETURNED);
    run_frame(&mut inst, Some(&waker));
    let before = wakes.load(std::sync::atomic::Ordering::SeqCst);

    let _sooner = inst
        .canvas
        .with_entered(|| reactive_core::run_after(DELAY / 10, || {}));
    assert!(
        wakes.load(std::sync::atomic::Ordering::SeqCst) > before,
        "a timer due before the one the host sleeps for wakes it"
    );
}

/// A blocking overlay over the whole guest that takes every pointer event.
struct Blanket;

impl ui_tree::OverlaySink for Blanket {
    fn content_rect(&self) -> Rect {
        Rect::new(0.0, 0.0, 40.0, 20.0)
    }
    fn dispatch(&self, _event: &Event) -> EventResult {
        EventResult::Handled
    }
}

#[test]
fn a_move_an_overlay_takes_still_reaches_the_content_under_it() {
    let (inst, _, moves) = waiting();
    let overlay = inst
        .canvas
        .with_entered(|| ui_tree::register_overlay(Rc::new(Blanket)));
    let moved = Event::PointerMoved {
        x: 10.0,
        y: 10.0,
        source: platform_core::PointerSource::Mouse,
    };
    assert!(inst.dispatch_overlays(&moved), "the overlay takes the move");
    assert_eq!(moves.get(), 1, "and the content hears it, covered");

    let pressed = Event::PointerPressed {
        x: 10.0,
        y: 10.0,
        button: platform_core::PointerButton::Primary,
        source: platform_core::PointerSource::Mouse,
    };
    assert!(inst.dispatch_overlays(&pressed));
    assert_eq!(moves.get(), 1, "what else it takes stays its own");

    inst.canvas.with_entered(|| {
        ui_tree::unregister_overlay(overlay);
    })
}

#[cfg(all(feature = "host", not(target_arch = "wasm32")))]
#[test]
fn the_host_follows_a_guest_deadline_without_arming_it_again() {
    let mirror = crate::host::TimerMirror::default();
    let due = web_time::Instant::now() + DELAY;
    mirror.follow(Some(due), Some(DELAY));
    let wait = reactive_core::until_next_timer().expect("the host's loop wakes for it");
    assert!(wait <= DELAY && wait > DELAY / 2, "{wait:?}");

    reactive_core::advance_timer_clock(DELAY / 2);
    mirror.follow(Some(due), Some(DELAY * 4));
    let wait = reactive_core::until_next_timer().expect("still waking for it");
    assert!(
        wait <= DELAY / 2,
        "the same deadline is not armed again: {wait:?}"
    );

    let later = due + DELAY;
    mirror.follow(Some(later), Some(DELAY * 3));
    let wait = reactive_core::until_next_timer().expect("a moved deadline is followed");
    assert!(wait > DELAY * 2, "{wait:?}");

    mirror.follow(None, None);
    assert_eq!(
        reactive_core::until_next_timer(),
        None,
        "nothing left to wake for"
    );
}

thread_local! {
    static PRESSES_HEARD: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    static APPS_DROPPED: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// A guest whose press handler panics, as one that writes a signal its own effect still borrows does, and which counts the presses it hears and the times it is dropped.
struct BorrowsTwice {
    stub: Stub,
    panics: bool,
}

impl BorrowsTwice {
    fn new(panics: bool) -> Self {
        Self {
            stub: Stub::new(),
            panics,
        }
    }
}

impl Drop for BorrowsTwice {
    fn drop(&mut self) {
        APPS_DROPPED.with(|dropped| dropped.set(dropped.get() + 1));
    }
}

impl EmbeddedApp for BorrowsTwice {
    fn build(&mut self) {
        self.stub.build();
    }
    fn layout_root(&self) -> NodeId {
        self.stub.layout_root()
    }
    fn view(&self) -> RenderNode {
        self.stub.view()
    }
    fn on_event(&mut self, event: &Event) -> EventResult {
        if matches!(event, Event::PointerPressed { .. }) {
            PRESSES_HEARD.with(|heard| heard.set(heard.get() + 1));
            if self.panics {
                panic!("RUNTIME is already borrowed");
            }
        }
        EventResult::Ignored
    }
    fn title(&self) -> String {
        "borrows twice".into()
    }
    fn id(&self) -> String {
        "borrows-twice".into()
    }
}

fn press() -> Event {
    Event::PointerPressed {
        x: 10.0,
        y: 10.0,
        button: platform_core::PointerButton::Primary,
        source: platform_core::PointerSource::Mouse,
    }
}

fn create_with(
    make: impl FnOnce(&[String]) -> Box<dyn EmbeddedApp>,
) -> (CallOutcome, Handle<GuestInstance>) {
    let args = wire_core::encode(&Vec::<String>::new());
    let mut instance = Handle::NULL;
    let outcome = unsafe { guest::create(LentBytes::lend(&args), &mut instance, make) };
    (outcome, instance)
}

/// What a panic in the guest's handler hands back instead of unwinding into the host's copy of std, which can only abort on it.
#[test]
fn a_panic_in_a_guest_entry_comes_back_as_its_report() {
    let (built, instance) = create_with(|_| Box::new(BorrowsTwice::new(true)));
    assert_eq!(built.status(), CallOutcome::RETURNED, "the guest builds");
    let pressed = wire_core::encode(&press());
    let mut handled = false;
    let outcome = unsafe {
        (TELAR_EMBED_ENTRIES.on_event)(instance, LentBytes::lend(&pressed), &mut handled)
    };
    let panic = unsafe { outcome.into_result() }.expect_err("the panic is caught");
    assert_eq!(panic.message, "RUNTIME is already borrowed");
    let location = panic.location.as_deref().expect("the hook saw where");
    assert!(location.starts_with(&format!("{}:", file!())), "{location}");
}

#[test]
fn a_guest_whose_build_panics_hands_back_no_instance() {
    let (outcome, instance) = create_with(|_| panic!("the guest could not find its assets"));
    let panic = unsafe { outcome.into_result() }.expect_err("the panic is caught");
    assert_eq!(panic.message, "the guest could not find its assets");
    assert!(instance.is_null());
}

#[cfg(all(feature = "host", not(target_arch = "wasm32")))]
unsafe extern "C" fn create_panicking(
    args: LentBytes,
    instance: *mut Handle<GuestInstance>,
) -> CallOutcome {
    unsafe { guest::create(args, instance, |_| Box::new(BorrowsTwice::new(true))) }
}

#[cfg(all(feature = "host", not(target_arch = "wasm32")))]
unsafe extern "C" fn create_fixed(
    args: LentBytes,
    instance: *mut Handle<GuestInstance>,
) -> CallOutcome {
    unsafe { guest::create(args, instance, |_| Box::new(BorrowsTwice::new(false))) }
}

#[cfg(all(feature = "host", not(target_arch = "wasm32")))]
unsafe extern "C" fn create_nothing(
    args: LentBytes,
    instance: *mut Handle<GuestInstance>,
) -> CallOutcome {
    unsafe {
        guest::create(args, instance, |_| {
            panic!("the guest could not find its assets")
        })
    }
}

/// A guest served by this process's own table, through the same catch and the same bookkeeping a dylib's go through. What stays untested here is the second copy of std a real dylib carries, which `telar`'s `guest_contract` suite loads.
#[cfg(all(feature = "host", not(target_arch = "wasm32")))]
fn embedded(create: CreateInstance) -> Result<LoadedEmbed, GuestPanic> {
    LoadedEmbed::start(
        EmbedEntries {
            create,
            ..TELAR_EMBED_ENTRIES
        },
        &[],
        None,
    )
}

/// A panic in an embedded guest's handler used to unwind into the host and abort it. Now the host is told what panicked and where, nothing more of that guest runs, and a fresh one takes its place.
#[cfg(all(feature = "host", not(target_arch = "wasm32")))]
#[test]
fn a_guest_that_panics_is_reported_and_never_called_again() {
    PRESSES_HEARD.with(|heard| heard.set(0));
    APPS_DROPPED.with(|dropped| dropped.set(0));
    let broken = embedded(create_panicking).expect("the guest loads");
    broken.relayout(40.0, 20.0);
    assert_eq!(broken.panic(), None, "precondition: a guest that runs");
    assert!(
        !broken.paint().is_empty(),
        "precondition: a guest that paints"
    );

    assert!(!broken.on_event(&press()));
    assert_eq!(PRESSES_HEARD.with(std::cell::Cell::get), 1);
    let panic = broken.panic().expect("the host is told the guest panicked");
    assert_eq!(panic.message, "RUNTIME is already borrowed");
    assert!(
        panic
            .location
            .as_deref()
            .is_some_and(|location| location.starts_with(file!())),
        "the report says where: {panic}"
    );

    assert!(!broken.on_event(&press()));
    assert!(!broken.dispatch_overlays(&press()));
    assert!(
        broken.paint().is_empty(),
        "a guest that panicked draws nothing"
    );
    assert_eq!(broken.title(), "");
    run_embedded_frame(&broken);
    assert_eq!(
        PRESSES_HEARD.with(std::cell::Cell::get),
        1,
        "nothing more of the guest that panicked runs"
    );
    drop(broken);
    assert_eq!(
        APPS_DROPPED.with(std::cell::Cell::get),
        0,
        "its instance is kept rather than released by code that already panicked"
    );

    let fresh = embedded(create_fixed).expect("a fresh guest loads");
    fresh.on_event(&press());
    assert_eq!(
        PRESSES_HEARD.with(std::cell::Cell::get),
        2,
        "the fresh guest hears the press"
    );
    assert_eq!(fresh.panic(), None);
    drop(fresh);
    assert_eq!(
        APPS_DROPPED.with(std::cell::Cell::get),
        1,
        "and is released as usual"
    );
}

#[cfg(all(feature = "host", not(target_arch = "wasm32")))]
#[test]
fn a_guest_whose_build_panics_is_refused_with_the_report() {
    let panic = embedded(create_nothing)
        .err()
        .expect("a guest that cannot build does not load");
    assert_eq!(panic.message, "the guest could not find its assets");
}

/// The host reads every answer the guest lends into values of its own: its metadata, the commands its title bar queued and the timer it waits on, on the host's clock.
#[cfg(all(feature = "host", not(target_arch = "wasm32")))]
#[test]
fn a_host_reads_what_a_guest_answers_into_its_own_values() {
    let embed = embedded(create_waiting).expect("the guest loads");
    assert_eq!(embed.title(), "waiting");
    assert_eq!(embed.id(), "waiting");
    assert_eq!(embed.icon(), None);
    assert_eq!(embed.clear_color(), None);
    assert!(embed.drain_window_commands().is_empty());
    let wait = embed.until_next_timer().expect("the guest's timer crosses");
    assert!(wait <= DELAY && wait > DELAY / 2, "{wait:?}");
    let due = embed.next_timer_due().expect("and its deadline");
    assert_eq!(
        embed.next_timer_due(),
        Some(due),
        "the same deadline reads the same on the host's clock"
    );
    let from_now = due.saturating_duration_since(Instant::now());
    assert!(from_now <= DELAY && from_now > DELAY / 2, "{from_now:?}");
    assert_eq!(embed.panic(), None);
}

#[cfg(all(feature = "host", not(target_arch = "wasm32")))]
unsafe extern "C" fn create_waiting(
    args: LentBytes,
    instance: *mut Handle<GuestInstance>,
) -> CallOutcome {
    let fired = Rc::new(std::cell::Cell::new(false));
    let moves = Rc::new(std::cell::Cell::new(0));
    unsafe { guest::create(args, instance, |_| Box::new(Waiting::new(&fired, &moves))) }
}

#[cfg(all(feature = "host", not(target_arch = "wasm32")))]
fn run_embedded_frame(embed: &LoadedEmbed) {
    let waker = platform_core::RedrawWaker::new(|| panic!("the host's loop is gone"));
    let mut redraw_requested = false;
    let mut ctx = AppCtx::new(&mut redraw_requested, Some(&waker), None, None);
    embed.on_frame(&mut ctx);
}

/// A guest that wakes the host's loop every frame.
#[cfg(all(feature = "host", not(target_arch = "wasm32")))]
struct WakesTheHost(Stub);

#[cfg(all(feature = "host", not(target_arch = "wasm32")))]
impl EmbeddedApp for WakesTheHost {
    fn build(&mut self) {
        self.0.build();
    }
    fn layout_root(&self) -> NodeId {
        self.0.layout_root()
    }
    fn view(&self) -> RenderNode {
        self.0.view()
    }
    fn on_event(&mut self, event: &Event) -> EventResult {
        self.0.on_event(event)
    }
    fn on_frame(&mut self, ctx: &mut AppCtx) {
        if let Some(waker) = ctx.redraw_waker() {
            waker.wake();
        }
        ctx.request_redraw();
    }
    fn title(&self) -> String {
        "wakes".into()
    }
    fn id(&self) -> String {
        "wakes".into()
    }
}

#[cfg(all(feature = "host", not(target_arch = "wasm32")))]
unsafe extern "C" fn create_waking(
    args: LentBytes,
    instance: *mut Handle<GuestInstance>,
) -> CallOutcome {
    unsafe { guest::create(args, instance, |_| Box::new(WakesTheHost(Stub::new()))) }
}

/// The waker the guest is handed is host code; a panic in it stays on the host's side instead of unwinding through the guest, and the redraw the guest asks for reaches the host's context.
#[cfg(all(feature = "host", not(target_arch = "wasm32")))]
#[test]
fn a_panic_in_the_host_waker_does_not_reach_the_guest() {
    let embed = embedded(create_waking).expect("the guest loads");
    let waker = platform_core::RedrawWaker::new(|| panic!("the host's loop is gone"));
    let mut redraw_requested = false;
    let mut ctx = AppCtx::new(&mut redraw_requested, Some(&waker), None, None);
    embed.on_frame(&mut ctx);
    assert_eq!(embed.panic(), None, "the guest kept running");
    assert!(redraw_requested, "its redraw reaches the host");
}

#[cfg(all(feature = "host", not(target_arch = "wasm32")))]
thread_local! {
    static CONVENTION_HANDED: std::cell::Cell<Option<KeyboardConvention>> = const { std::cell::Cell::new(None) };
    static HANDED_BEFORE_BUILD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static SERVED_NOTHING: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

#[cfg(all(feature = "host", not(target_arch = "wasm32")))]
unsafe extern "C" fn record_services(services: HostServices) -> CallOutcome {
    CONVENTION_HANDED.set(unsafe { services.ask(services.keyboard_convention, &()) });
    SERVED_NOTHING.set(Some(services.data.is_null()));
    CallOutcome::OK
}

#[cfg(all(feature = "host", unix))]
unsafe extern "C" fn create_after_services(
    args: LentBytes,
    instance: *mut Handle<GuestInstance>,
) -> CallOutcome {
    HANDED_BEFORE_BUILD.set(CONVENTION_HANDED.get().is_some());
    unsafe { guest::create(args, instance, |_| Box::new(Stub::new())) }
}

#[cfg(all(feature = "host", unix))]
extern "C" fn does_nothing() -> CallOutcome {
    CallOutcome::OK
}

fn foreign_convention() -> KeyboardConvention {
    match KeyboardConvention::native() {
        KeyboardConvention::Mac => KeyboardConvention::Pc,
        KeyboardConvention::Pc => KeyboardConvention::Mac,
    }
}

/// A guest carries its own copy of the convention, which left alone answers for the system it was compiled on: a terminal host on a Mac matches Ctrl while the guest would show ⌘.
#[cfg(all(feature = "host", unix))]
#[test]
fn a_guest_is_handed_the_host_services_before_it_is_built() {
    let host = foreign_convention();
    platform_core::set_keyboard_convention(host);
    let this_process = libloading::Library::from(libloading::os::unix::Library::this());
    let embed = LoadedEmbed::start(
        EmbedEntries {
            create: create_after_services,
            lifecycle: Lifecycle {
                install_services: record_services,
                loaded: does_nothing,
                unloading: does_nothing,
                quiesce: does_nothing,
                ..TELAR_EMBED_ENTRIES.lifecycle
            },
            ..TELAR_EMBED_ENTRIES
        },
        &[],
        Some(this_process),
    )
    .expect("the guest loads");
    assert_eq!(CONVENTION_HANDED.get(), Some(host));
    assert!(
        HANDED_BEFORE_BUILD.get(),
        "the guest's build already shows shortcuts the host's way"
    );
    drop(embed);
}

/// A table this process serves itself already shares every store there is, the one font database included: handed it, its shaper would defer to itself.
#[cfg(all(feature = "host", not(target_arch = "wasm32")))]
#[test]
fn a_guest_of_this_process_is_served_nothing() {
    let embed = LoadedEmbed::start(
        EmbedEntries {
            lifecycle: Lifecycle {
                install_services: record_services,
                ..TELAR_EMBED_ENTRIES.lifecycle
            },
            ..TELAR_EMBED_ENTRIES
        },
        &[],
        None,
    )
    .expect("the guest loads");
    assert_eq!(SERVED_NOTHING.get(), Some(true));
    drop(embed);
}

#[test]
fn the_exported_table_installs_the_convention_it_is_handed() {
    let handed = foreign_convention();
    platform_core::set_keyboard_convention(handed);
    let served = services_core::guest::ServedServices::installed(None);
    platform_core::set_keyboard_convention(KeyboardConvention::native());
    let full = served.table();
    let only_the_convention = HostServices {
        data: full.data,
        keyboard_convention: full.keyboard_convention,
        ..HostServices::NONE
    };
    let outcome = unsafe { (TELAR_EMBED_ENTRIES.lifecycle.install_services)(only_the_convention) };
    assert_eq!(outcome.status(), CallOutcome::RETURNED);
    assert_eq!(platform_core::keyboard_convention(), handed);
}

#[cfg(all(feature = "host", unix))]
thread_local! {
    static LIFE: std::cell::RefCell<Vec<&'static str>> = const { std::cell::RefCell::new(Vec::new()) };
}

#[cfg(all(feature = "host", unix))]
fn live(call: &'static str) -> CallOutcome {
    LIFE.with_borrow_mut(|calls| calls.push(call));
    CallOutcome::OK
}

#[cfg(all(feature = "host", unix))]
extern "C" fn live_loaded() -> CallOutcome {
    live("loaded")
}

#[cfg(all(feature = "host", unix))]
extern "C" fn live_unloading() -> CallOutcome {
    live("unloading")
}

#[cfg(all(feature = "host", unix))]
extern "C" fn live_quiesce() -> CallOutcome {
    live("quiesce")
}

#[cfg(all(feature = "host", unix))]
extern "C" fn serves_nothing(_services: HostServices) -> CallOutcome {
    CallOutcome::OK
}

/// Held with this process as its library, so dropping it runs the unload path a built library's does.
#[cfg(all(feature = "host", unix))]
fn embedded_as_loaded(create: CreateInstance) -> Result<LoadedEmbed, GuestPanic> {
    LIFE.with_borrow_mut(Vec::clear);
    let this_process = libloading::Library::from(libloading::os::unix::Library::this());
    LoadedEmbed::start(
        EmbedEntries {
            create,
            lifecycle: Lifecycle {
                loaded: live_loaded,
                unloading: live_unloading,
                quiesce: live_quiesce,
                install_services: serves_nothing,
                ..TELAR_EMBED_ENTRIES.lifecycle
            },
            ..TELAR_EMBED_ENTRIES
        },
        &[],
        Some(this_process),
    )
}

#[cfg(all(feature = "host", unix))]
#[test]
fn a_guest_is_released_told_it_is_unloading_then_quiesced_before_its_library_is_closed() {
    APPS_DROPPED.with(|dropped| dropped.set(0));
    drop(embedded_as_loaded(create_fixed).expect("the guest loads"));
    assert_eq!(APPS_DROPPED.with(std::cell::Cell::get), 1);
    assert_eq!(LIFE.take(), ["loaded", "unloading", "quiesce"]);
}

/// Its worker threads may still be running its code, so the one call a guest that panicked still gets is the one that joins them.
#[cfg(all(feature = "host", unix))]
#[test]
fn a_guest_that_panicked_is_still_quiesced_before_its_library_is_closed() {
    APPS_DROPPED.with(|dropped| dropped.set(0));
    let broken = embedded_as_loaded(create_panicking).expect("the guest loads");
    broken.relayout(40.0, 20.0);
    broken.on_event(&press());
    assert!(broken.panic().is_some(), "precondition: it panicked");
    drop(broken);
    assert_eq!(APPS_DROPPED.with(std::cell::Cell::get), 0);
    assert_eq!(LIFE.take(), ["loaded", "quiesce"]);
}

#[cfg(all(feature = "host", unix))]
#[test]
fn a_guest_whose_build_panics_is_still_quiesced_before_its_library_is_closed() {
    let refused = embedded_as_loaded(create_nothing);
    assert!(refused.is_err(), "precondition: it does not load");
    assert_eq!(LIFE.take(), ["loaded", "quiesce"]);
}

struct PanicsWhenTicked;

impl motion_core::Tickable for PanicsWhenTicked {
    fn tick(&self, _now: Instant, _scale: f32) {
        panic!("an animation callback panicked");
    }
    fn is_settled(&self) -> bool {
        false
    }
}

#[test]
fn an_animation_that_panics_mid_tick_leaves_later_flushes_working() {
    let inst = EmbedInstance::new(Box::new(Stub::new()));
    let animation: std::rc::Rc<dyn motion_core::Tickable> = std::rc::Rc::new(PanicsWhenTicked);
    inst.canvas.with_entered(|| {
        motion_core::register(motion_core::next_id(), std::rc::Rc::downgrade(&animation))
    });

    let ticked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        inst.motion_tick(Instant::now())
    }));
    assert!(ticked.is_err(), "precondition: the tick panicked");
    drop(animation);

    let count = reactive_core::signal(0);
    let seen = std::rc::Rc::new(std::cell::Cell::new(0));
    let into = std::rc::Rc::clone(&seen);
    let _effect = reactive_core::effect(move || into.set(count.get()));
    count.set(3);
    assert_eq!(
        seen.get(),
        3,
        "the batch the tick opened ended with the unwind, so the write flushed"
    );
}
