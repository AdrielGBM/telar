use super::*;
use layout_core::LayoutStyle;
use platform_core::{Key, ModifiersState};
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

// No crate here links a guest cdylib, so this is the only place the expansion is ever compiled: without it, adding a vtable field type-checks and breaks every guest at load time.
crate::embed!(|_args: &[String]| -> Box<dyn EmbeddedApp> { Box::new(Stub::new()) });

#[test]
fn the_export_macro_builds_a_vtable_at_the_current_abi() {
    assert_eq!(_rsx_embed_vtable.abi, TELAR_EMBED_ABI);
    let inst = unsafe { (_rsx_embed_vtable.create)(&[]) };
    assert!(!inst.is_null(), "the exported vtable built an instance");
    assert_eq!(unsafe { (_rsx_embed_vtable.id)(inst) }, "stub");
    let dark = SystemPreferences {
        color_scheme: Some(platform_core::ColorScheme::Dark),
        reduced_motion: Some(true),
        ..SystemPreferences::default()
    };
    unsafe { (_rsx_embed_vtable.set_system_preferences)(inst, &dark) };
    assert_eq!(
        preferences_core::system_preferences(),
        dark,
        "the whole snapshot crosses, not only the scheme"
    );
    preferences_core::set_system_preferences(SystemPreferences::default());
    unsafe { (_rsx_embed_vtable.destroy)(inst) };
}

// These deliberately never observe on the caller's behalf, so they fail the moment either observe call leaves `EmbedInstance::on_event`.
#[test]
fn a_guest_records_the_modifiers_it_is_handed() {
    let mut inst = EmbedInstance::new(Box::new(Stub::new()));
    let _g = inst.canvas.enter();
    assert_eq!(ui_core::modifiers(), ModifiersState::default());
    drop(_g);

    inst.on_event(&Event::ModifiersChanged { modifiers: shift() });

    let _g = inst.canvas.enter();
    assert!(
        ui_core::modifiers().is_shift,
        "a shift-drag inside a guest is indistinguishable from a plain one without this"
    );
}

#[test]
fn an_overlay_event_reaches_the_registry_too() {
    let inst = EmbedInstance::new(Box::new(Stub::new()));
    inst.dispatch_overlays(&Event::ModifiersChanged { modifiers: shift() });

    let _g = inst.canvas.enter();
    assert!(
        ui_core::modifiers().is_shift,
        "an overlay event reaches the shared registry"
    );
}

#[test]
fn a_press_answers_for_one_frame_and_end_frame_closes_it() {
    let mut inst = EmbedInstance::new(Box::new(Stub::new()));
    inst.on_event(&Event::KeyPressed {
        key: Key::Char('c'),
        modifiers: ModifiersState::default(),
        unmodified: None,
    });

    {
        let _g = inst.canvas.enter();
        assert!(
            ui_core::key_pressed(&Key::Char('c')),
            "the press answers for the frame it arrived in"
        );
    }
    inst.end_frame();
    let _g = inst.canvas.enter();
    assert!(
        !ui_core::key_pressed(&Key::Char('c')),
        "without end_frame the press answers forever, not for its frame"
    );
    assert!(
        ui_core::key_held(&Key::Char('c')),
        "held is not what end_frame clears"
    );
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

#[test]
fn composite_translates_into_the_sub_rect_and_clips_to_it() {
    let rect = Rect::new(10.0, 20.0, 100.0, 50.0);
    let node = composite(
        rect,
        0,
        vec![DrawCommand::Rect {
            rect: Rect::new(0.0, 0.0, 5.0, 5.0),
            style: std::sync::Arc::new(RectStyle::default()),
        }],
    );
    match node {
        RenderNode::Clip {
            rect: clip,
            children,
            ..
        } => {
            assert_eq!(clip, rect, "clipped to the host's sub-rect");
            assert!(
                !children.is_empty(),
                "the composite wraps the guest's own nodes"
            );
        }
        _ => panic!("expected the guest's frame to be wrapped in a clip"),
    }
}

// What a guest built before ABI 2 exports: a shorter table whose only field the host may read is the version at offset 0.
#[cfg(feature = "host")]
#[repr(C)]
struct AbiOneTable {
    abi: u32,
    _create: usize,
}

#[cfg(feature = "host")]
static ABI_ONE: AbiOneTable = AbiOneTable { abi: 1, _create: 0 };

#[cfg(feature = "host")]
#[test]
fn a_guest_built_for_abi_one_is_refused_before_its_table_is_read() {
    let refused = unsafe { crate::host::read_vtable((&raw const ABI_ONE).cast::<EmbedVTable>()) };
    let mismatch = refused.err().expect("an ABI 1 table must not load");
    assert_eq!(mismatch.guest, 1);
    assert_eq!(
        mismatch.to_string(),
        format!("guest built for ABI 1, host is ABI {TELAR_EMBED_ABI}")
    );
}

#[cfg(feature = "host")]
#[test]
fn a_guest_built_for_this_abi_is_read_whole() {
    let read = unsafe { crate::host::read_vtable(&raw const _rsx_embed_vtable) }
        .expect("the current table loads");
    assert_eq!(read.abi, TELAR_EMBED_ABI);
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
    run_frame(&mut inst, Some(&waker));
    let before = wakes.load(std::sync::atomic::Ordering::SeqCst);

    let _sooner = {
        let _g = inst.canvas.enter();
        reactive_core::run_after(DELAY / 10, || {})
    };
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
    let overlay = {
        let _g = inst.canvas.enter();
        ui_tree::register_overlay(Rc::new(Blanket))
    };
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

    let _g = inst.canvas.enter();
    ui_tree::unregister_overlay(overlay);
}

#[cfg(feature = "host")]
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
