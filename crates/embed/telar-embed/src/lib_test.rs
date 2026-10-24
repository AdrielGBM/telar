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
    let inst =
        unsafe { (_rsx_embed_vtable.create)(&[]) }.expect("the exported vtable built an instance");
    assert_eq!(
        unsafe { (_rsx_embed_vtable.id)(inst) },
        Ok("stub".to_owned())
    );
    let dark = SystemPreferences {
        color_scheme: Some(platform_core::ColorScheme::Dark),
        reduced_motion: Some(true),
        ..SystemPreferences::default()
    };
    unsafe { (_rsx_embed_vtable.set_system_preferences)(inst, &dark) }.expect("no panic");
    assert_eq!(
        preferences_core::system_preferences(),
        dark,
        "the whole snapshot crosses, not only the scheme"
    );
    preferences_core::set_system_preferences(SystemPreferences::default());
    unsafe { (_rsx_embed_vtable.destroy)(inst) }.expect("no panic");
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
#[cfg(all(feature = "host", not(target_arch = "wasm32")))]
#[repr(C)]
struct AbiOneTable {
    abi: u32,
    _create: usize,
}

#[cfg(all(feature = "host", not(target_arch = "wasm32")))]
static ABI_ONE: AbiOneTable = AbiOneTable { abi: 1, _create: 0 };

#[cfg(all(feature = "host", not(target_arch = "wasm32")))]
#[test]
fn a_guest_built_for_abi_one_is_refused_before_its_table_is_read() {
    let refused = unsafe { crate::host::read_vtable((&raw const ABI_ONE).cast::<EmbedVTable>()) };
    let mismatch = refused.err().expect("an ABI 1 table must not load");
    assert_eq!(mismatch, crate::host::AbiMismatch::Version { guest: 1 });
    assert_eq!(
        mismatch.to_string(),
        format!("guest built for ABI 1, host is ABI {TELAR_EMBED_ABI}")
    );
}

#[cfg(all(feature = "host", not(target_arch = "wasm32")))]
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

/// What a panic in the guest's handler hands back instead of unwinding into the host's copy of std, which can only abort on it.
#[test]
fn a_panic_in_a_guest_export_comes_back_as_its_report() {
    let inst = __embed_create(|| Box::new(BorrowsTwice::new(true))).expect("the guest builds");
    let panic = unsafe { __embed_on_event(inst, &press()) }.expect_err("the panic is caught");
    assert_eq!(panic.message, "RUNTIME is already borrowed");
    let location = panic.location.as_deref().expect("the hook saw where");
    assert!(location.starts_with(&format!("{}:", file!())), "{location}");
}

#[test]
fn a_guest_whose_build_panics_hands_back_no_instance() {
    let panic = __embed_create(|| panic!("the guest could not find its assets"))
        .expect_err("the panic is caught");
    assert_eq!(panic.message, "the guest could not find its assets");
}

#[cfg(all(feature = "host", not(target_arch = "wasm32")))]
unsafe fn create_panicking(_args: &[String]) -> GuestResult<*mut EmbedInstance> {
    __embed_create(|| Box::new(BorrowsTwice::new(true)))
}

#[cfg(all(feature = "host", not(target_arch = "wasm32")))]
unsafe fn create_fixed(_args: &[String]) -> GuestResult<*mut EmbedInstance> {
    __embed_create(|| Box::new(BorrowsTwice::new(false)))
}

#[cfg(all(feature = "host", not(target_arch = "wasm32")))]
unsafe fn create_nothing(_args: &[String]) -> GuestResult<*mut EmbedInstance> {
    __embed_create(|| panic!("the guest could not find its assets"))
}

/// A guest served by this process's own table, through the same catch and the same bookkeeping a dylib's go through. What stays untested here is the second copy of std a real dylib carries.
#[cfg(all(feature = "host", not(target_arch = "wasm32")))]
fn embedded(
    create: unsafe fn(&[String]) -> GuestResult<*mut EmbedInstance>,
) -> Result<LoadedEmbed, GuestPanic> {
    LoadedEmbed::start(
        EmbedVTable {
            create,
            .._rsx_embed_vtable
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
        "its instance is kept rather than destroyed by code that already panicked"
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
        "and is destroyed as usual"
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
    }
    fn title(&self) -> String {
        "wakes".into()
    }
    fn id(&self) -> String {
        "wakes".into()
    }
}

#[cfg(all(feature = "host", not(target_arch = "wasm32")))]
unsafe fn create_waking(_args: &[String]) -> GuestResult<*mut EmbedInstance> {
    __embed_create(|| Box::new(WakesTheHost(Stub::new())))
}

/// The waker the guest is handed is host code; a panic in it stays on the host's side instead of unwinding through the guest.
#[cfg(all(feature = "host", not(target_arch = "wasm32")))]
#[test]
fn a_panic_in_the_host_waker_does_not_reach_the_guest() {
    let embed = embedded(create_waking).expect("the guest loads");
    run_embedded_frame(&embed);
    assert_eq!(embed.panic(), None, "the guest kept running");
}

#[cfg(all(feature = "host", not(target_arch = "wasm32")))]
#[test]
fn a_guest_laid_out_differently_is_refused_with_what_to_do() {
    let mut table = _rsx_embed_vtable;
    table.layout ^= 1;
    let refused = unsafe { crate::host::read_vtable(&raw const table) };
    let mismatch = refused.err().expect("another layout must not load");
    let message = mismatch.to_string();
    assert!(message.contains("restart the session"), "{message}");
    assert!(message.contains("matching features"), "{message}");
}

#[cfg(all(feature = "host", not(target_arch = "wasm32")))]
thread_local! {
    static CONVENTION_HANDED: std::cell::Cell<Option<KeyboardConvention>> = const { std::cell::Cell::new(None) };
    static HANDED_BEFORE_BUILD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(all(feature = "host", not(target_arch = "wasm32")))]
unsafe fn record_convention(convention: KeyboardConvention) -> GuestResult<()> {
    CONVENTION_HANDED.set(Some(convention));
    Ok(())
}

#[cfg(all(feature = "host", not(target_arch = "wasm32")))]
unsafe fn create_after_convention(_args: &[String]) -> GuestResult<*mut EmbedInstance> {
    HANDED_BEFORE_BUILD.set(CONVENTION_HANDED.get().is_some());
    __embed_create(|| Box::new(Stub::new()))
}

fn foreign_convention() -> KeyboardConvention {
    match KeyboardConvention::native() {
        KeyboardConvention::Mac => KeyboardConvention::Pc,
        KeyboardConvention::Pc => KeyboardConvention::Mac,
    }
}

/// A guest carries its own copy of the convention, which left alone answers for the system it was compiled on: a terminal host on a Mac matches Ctrl while the guest would show ⌘.
#[cfg(all(feature = "host", not(target_arch = "wasm32")))]
#[test]
fn a_guest_is_handed_the_host_keyboard_convention_before_it_is_built() {
    let host = foreign_convention();
    platform_core::set_keyboard_convention(host);
    let embed = LoadedEmbed::start(
        EmbedVTable {
            create: create_after_convention,
            set_keyboard_convention: record_convention,
            .._rsx_embed_vtable
        },
        &[],
        None,
    )
    .expect("the guest loads");
    assert_eq!(CONVENTION_HANDED.get(), Some(host));
    assert!(
        HANDED_BEFORE_BUILD.get(),
        "the guest's build already shows shortcuts the host's way"
    );
    drop(embed);
}

#[test]
fn the_exported_table_installs_the_convention_it_is_handed() {
    let handed = foreign_convention();
    unsafe { (_rsx_embed_vtable.set_keyboard_convention)(handed) }.expect("no panic");
    assert_eq!(platform_core::keyboard_convention(), handed);
}

#[cfg(all(feature = "host", not(target_arch = "wasm32")))]
thread_local! {
    static FONTS_HANDED: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

#[cfg(all(feature = "host", not(target_arch = "wasm32")))]
unsafe fn record_fonts(host: Option<Arc<dyn renderer_core::FontHost>>) -> GuestResult<()> {
    FONTS_HANDED.set(Some(host.is_some()));
    Ok(())
}

/// A table this process serves itself already sets text in the one font database there is: handed it, its shaper would defer to itself.
#[cfg(all(feature = "host", not(target_arch = "wasm32")))]
#[test]
fn a_guest_of_this_process_is_handed_no_font_database_to_defer_to() {
    let embed = LoadedEmbed::start(
        EmbedVTable {
            defer_fonts: record_fonts,
            .._rsx_embed_vtable
        },
        &[],
        None,
    )
    .expect("the guest loads");
    assert_eq!(FONTS_HANDED.get(), Some(false));
    drop(embed);
}

#[cfg(all(feature = "host", unix))]
thread_local! {
    static LIFE: std::cell::RefCell<Vec<&'static str>> = const { std::cell::RefCell::new(Vec::new()) };
}

#[cfg(all(feature = "host", unix))]
fn live(call: &'static str) -> GuestResult<()> {
    LIFE.with_borrow_mut(|calls| calls.push(call));
    Ok(())
}

/// Held with this process as its library, so dropping it runs the unload path a built library's does.
#[cfg(all(feature = "host", unix))]
fn embedded_as_loaded(
    create: unsafe fn(&[String]) -> GuestResult<*mut EmbedInstance>,
) -> Result<LoadedEmbed, GuestPanic> {
    LIFE.with_borrow_mut(Vec::clear);
    let this_process = libloading::Library::from(libloading::os::unix::Library::this());
    LoadedEmbed::start(
        EmbedVTable {
            create,
            loaded: || live("loaded"),
            unloading: || live("unloading"),
            quiesce: || live("quiesce"),
            defer_fonts: |_| Ok(()),
            .._rsx_embed_vtable
        },
        &[],
        Some(this_process),
    )
}

#[cfg(all(feature = "host", unix))]
#[test]
fn a_guest_is_destroyed_released_then_quiesced_before_its_library_is_closed() {
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
