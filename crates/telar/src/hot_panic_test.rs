//! End-to-end tests of guests loaded from `tests/fixtures/hot-panic-guest`, a cdylib with its own copy of std, which they build under `target/tmp`: run them with `cargo test -p telar --features dev --test hot_panic -- --ignored`.

#![cfg(not(target_arch = "wasm32"))]

#[path = "guest_fixture.rs"]
mod guest_fixture;

use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use telar::hot::{HotApp, load_hot_app};
use telar::{
    AppCtx, AppRuntime, Chord, Color, Event, EventResult, KeyboardConvention, PointerButton,
    PointerSource, Size, TextStyle, set_keyboard_convention,
};

/// One at a time: both count what the process has mapped and what sits in the build directory.
static SERIAL: Mutex<()> = Mutex::new(());

const FIXTURE_SOURCE: &str = include_str!("../tests/fixtures/hot-panic-guest/src/lib.rs");

#[test]
#[ignore = "builds a cdylib against this checkout: run it with `cargo test -p telar --features dev --test hot_panic -- --ignored`"]
fn a_panic_in_a_hot_guest_is_reported_and_the_next_build_takes_over() {
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    let library = build_fixture();
    load_the_host_fonts();

    let mut broken = load(&library).expect("the guest loads");
    assert_eq!(
        broken.take_panic(),
        None,
        "precondition: a guest that starts"
    );
    assert!(runs_a_frame(&mut broken), "precondition: its frame runs");
    let mut tree = broken.mount();

    tree.on_event(&press());
    let report = broken
        .take_panic()
        .expect("the host is told the guest panicked");
    assert!(
        report.starts_with(&format!("panicked at src/lib.rs:{}:", panic_line())),
        "the report says where: {report}"
    );
    assert!(
        report.ends_with("RUNTIME is already borrowed by the press handler"),
        "the report says what: {report}"
    );

    tree.on_event(&press());
    assert!(
        !runs_a_frame(&mut broken),
        "nothing more of the guest that panicked runs"
    );
    drop(tree);

    let mut next = load(&library).expect("the next build loads beside the one still mapped");
    assert!(runs_a_frame(&mut next), "the next build runs");
    assert_eq!(
        next.take_panic(),
        None,
        "and shares nothing with the guest that panicked"
    );
    drop(broken);
    assert!(
        runs_a_frame(&mut next),
        "unmapping the one that panicked leaves it running"
    );

    let copies = std::fs::read_dir(library.parent().expect("a directory"))
        .expect("read the build directory")
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with(".guest-"))
        .count();
    assert_eq!(copies, 0, "every private copy is unlinked once opened");
}

#[test]
#[ignore = "builds a cdylib against this checkout: run it with `cargo test -p telar --features dev --test hot_panic -- --ignored`"]
#[cfg(target_os = "linux")]
fn a_hot_guest_that_ran_frames_is_unmapped_when_dropped() {
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    let library = build_fixture();
    let mut app = load(&library).expect("the guest loads");
    assert!(runs_a_frame(&mut app), "precondition: its frame runs");
    assert_eq!(mapped_guests(), 1, "precondition: it is mapped");
    drop(app);
    assert_eq!(
        mapped_guests(),
        0,
        "calling in through its entries leaves nothing that pins the library"
    );
}

#[test]
#[ignore = "builds a cdylib against this checkout: run it with `cargo test -p telar --features dev --test hot_panic -- --ignored`"]
#[cfg(target_os = "linux")]
fn a_hot_guest_that_mounted_a_tree_is_unmapped_when_dropped() {
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    let library = build_fixture();
    run_a_session(&library);
    assert_eq!(
        mapped_guests(),
        0,
        "building, painting and driving a tree leaves nothing that pins the library"
    );
}

/// `mount` hands out a `'static` tree, so safe code can drop the app first: the tree keeps its guest mapped, still drives and paints it, and the guest is unmapped once the tree goes too.
#[test]
#[ignore = "builds a cdylib against this checkout: run it with `cargo test -p telar --features dev --test hot_panic -- --ignored`"]
#[cfg(target_os = "linux")]
fn a_tree_that_outlives_its_hot_app_keeps_its_guest_mapped() {
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    let library = build_fixture();
    load_the_host_fonts();
    let mut app = load(&library).expect("the guest loads");
    app.set_surface_size(Size::new(320.0, 200.0));
    let mut tree = app.mount();
    app.relayout();
    assert_eq!(app.take_panic(), None, "precondition: the tree mounts");
    drop(app);
    assert_eq!(mapped_guests(), 1, "the tree keeps its guest mapped");
    assert_eq!(
        tree.on_event(&hover()),
        EventResult::Handled,
        "the tree still takes events"
    );
    assert!(!tree.frame().is_empty(), "the tree still paints");
    tree.end_frame();
    drop(tree);
    assert_eq!(mapped_guests(), 0, "the last holder unmaps it");
}

#[test]
#[ignore = "builds a cdylib against this checkout: run it with `cargo test -p telar --features dev --test hot_panic -- --ignored`"]
#[cfg(target_os = "linux")]
fn reloading_a_hot_guest_again_and_again_keeps_one_mapped() {
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    let library = build_fixture();
    for _ in 0..5 {
        run_a_session(&library);
    }
    let mut last = load(&library).expect("the guest loads");
    assert!(runs_a_frame(&mut last), "precondition: its frame runs");
    assert_eq!(
        mapped_guests(),
        1,
        "only the build in use is mapped, however many came before it"
    );
}

/// What each build held, its reactive runtime, the registries its tree filled and the baked assets its markup read among it, is freed as it is unloaded rather than left in the memory the host keeps: a session is a long run of reloads, and what one leaks every one does. Its two small baked SVGs alone kept about 3 KiB a reload while they were `LazyLock`s, which this bound refuses.
#[test]
#[ignore = "builds a cdylib against this checkout: run it with `cargo test -p telar --features dev --test hot_panic -- --ignored`"]
#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn reloading_a_hot_guest_again_and_again_frees_what_each_build_held() {
    const WARM_UP: usize = 3;
    const RELOADS: usize = 36;
    const TOLERANCE_PER_RELOAD: usize = 4 * 1024;
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    let library = build_fixture();
    for _ in 0..WARM_UP {
        run_a_session(&library);
    }
    let before = heap_in_use();
    for _ in 0..RELOADS {
        run_a_session(&library);
    }
    let grown = heap_in_use().saturating_sub(before);
    assert!(
        grown / RELOADS < TOLERANCE_PER_RELOAD,
        "{RELOADS} reloads left {grown} bytes behind, {} per reload",
        grown / RELOADS
    );
}

/// The host draws what the guest lays out, so a face the app adds while it runs must land in the host's database and not only in the guest's copy of it, or the host draws that text in a face it does not have; and it must outlive the build that added it, whose image, where an embedded face's bytes are, is unmapped as the next one takes over.
#[test]
#[ignore = "builds a cdylib against this checkout: run it with `cargo test -p telar --features dev --test hot_panic -- --ignored`"]
fn a_face_a_hot_guest_adds_while_it_runs_reaches_the_host() {
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    let library = build_fixture();
    load_the_host_fonts();
    assert!(
        !telar::font_family_available("Arriving Face"),
        "precondition: the host has not loaded it"
    );
    let mut app = load(&library).expect("the guest loads");
    let mut tree = app.mount();
    assert_eq!(
        tree.on_event(&secondary_press()),
        EventResult::Handled,
        "precondition: the guest adds the face"
    );
    assert_eq!(app.take_panic(), None, "precondition: nothing panicked");
    assert!(
        telar::font_family_available("Arriving Face"),
        "the host can draw in it"
    );
    drop(tree);
    drop(app);
    telar::install_default_text_metrics();
    let arriving = TextStyle::new(14.0, Color::BLACK).with_font_family("Arriving Face");
    let (width, _) = telar::measure_text("Arrived", None, f32::INFINITY, &arriving);
    assert!(
        width > 0.0,
        "the host still sets text in it once the guest is unloaded"
    );
}

/// The guest's copy of the convention answers for the system it was compiled on until the host hands over its own, which is what a terminal host on a Mac needs: it matches Ctrl, and a guest left alone would show ⌘.
#[test]
#[ignore = "builds a cdylib against this checkout: run it with `cargo test -p telar --features dev --test hot_panic -- --ignored`"]
fn a_hot_guest_shows_shortcuts_for_the_keyboard_its_host_matches() {
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    let library = build_fixture();
    for convention in [KeyboardConvention::Mac, KeyboardConvention::Pc] {
        set_keyboard_convention(convention);
        let app = load(&library).expect("the guest loads");
        let title = app
            .window_config()
            .expect("the guest titles its window with a shortcut")
            .title;
        let save = Chord::parse("Mod+S").expect("the shortcut parses");
        assert_eq!(title, save.text(convention), "{convention:?}");
    }
}

/// A guest that panicked is never told it is unloading, yet a task it left running is still code of its library: the host joins it before it closes the library under it.
#[test]
#[ignore = "builds a cdylib against this checkout: run it with `cargo test -p telar --features dev --test hot_panic -- --ignored`"]
#[cfg(target_os = "linux")]
fn a_hot_guest_that_panicked_with_a_task_running_is_unmapped_once_the_task_is_joined() {
    const GUEST_TASK_RUNS_FOR: std::time::Duration = std::time::Duration::from_millis(300);
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    let library = build_fixture();
    load_the_host_fonts();
    let mut app = load(&library).expect("the guest loads");
    let mut tree = app.mount();
    let pressed = std::time::Instant::now();
    tree.on_event(&auxiliary_press());
    assert!(app.take_panic().is_some(), "precondition: it panicked");
    drop(tree);
    assert_eq!(mapped_guests(), 1, "precondition: it is mapped");
    drop(app);
    assert!(
        pressed.elapsed() >= GUEST_TASK_RUNS_FOR,
        "its task ran to its end before the library was closed"
    );
    assert_eq!(mapped_guests(), 0, "and then the library is unmapped");
}

/// A task started from the host's own thread, the UI thread of a running app, which never exits: its worker is a thread the host started, so nothing of the guest's std is left on that thread to keep the library mapped.
#[test]
#[ignore = "builds a cdylib against this checkout: run it with `cargo test -p telar --features dev --test hot_panic -- --ignored`"]
#[cfg(target_os = "linux")]
fn a_hot_guest_that_started_a_task_from_the_host_thread_is_unmapped_when_dropped() {
    const GUEST_TASK_RUNS_FOR: std::time::Duration = std::time::Duration::from_millis(300);
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    let library = build_fixture();
    load_the_host_fonts();
    let mut app = load(&library).expect("the guest loads");
    let mut tree = app.mount();
    let released = std::time::Instant::now();
    assert_eq!(
        tree.on_event(&secondary_release()),
        EventResult::Handled,
        "precondition: the guest starts a task"
    );
    assert_eq!(app.take_panic(), None, "precondition: nothing panicked");
    drop(tree);
    assert_eq!(mapped_guests(), 1, "precondition: it is mapped");
    drop(app);
    assert!(
        released.elapsed() >= GUEST_TASK_RUNS_FOR,
        "its task ran to its end before the library was closed"
    );
    assert_eq!(
        mapped_guests(),
        0,
        "the thread that started the task keeps nothing of the guest's"
    );
}

/// The limit the contract documents: a guest that calls `std::thread::spawn` itself spawns with its own std, which keeps its library mapped for as long as the thread that spawned lives, and until the next guest is closed after that.
#[test]
#[ignore = "builds a cdylib against this checkout: run it with `cargo test -p telar --features dev --test hot_panic -- --ignored`"]
#[cfg(target_os = "linux")]
fn a_hot_guest_that_spawns_with_its_own_std_is_unmapped_only_once_the_spawning_thread_exits() {
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    let library = build_fixture();
    load_the_host_fonts();
    // Driven from a thread that then ends, as the app's own spawn pins the library to the thread that spawned.
    std::thread::scope(|scope| {
        scope
            .spawn(|| {
                let mut app = load(&library).expect("the guest loads");
                let mut tree = app.mount();
                assert_eq!(
                    tree.on_event(&auxiliary_release()),
                    EventResult::Handled,
                    "precondition: the guest spawns"
                );
                assert_eq!(app.take_panic(), None, "precondition: nothing panicked");
                drop(tree);
                drop(app);
                assert_eq!(
                    mapped_guests(),
                    1,
                    "the spawning thread keeps it mapped while it lives"
                );
            })
            .join()
            .expect("the host's thread runs to its end")
    });
    // glibc only unmaps a library left pinned at its `dlclose` on a later `dlclose`, which the next reload's is.
    drop(load(&library).expect("the next build loads"));
    assert_eq!(mapped_guests(), 0, "and then the library is unmapped");
}

/// A guest whose threads could not be joined may still be running its code, so it stays mapped for the rest of the process, and the host keeps running.
#[test]
#[ignore = "builds a cdylib against this checkout: run it with `cargo test -p telar --features dev --test hot_panic -- --ignored`"]
#[cfg(target_os = "linux")]
fn a_hot_guest_that_cannot_be_quiesced_stays_mapped() {
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    let library = build_fixture();
    load_the_host_fonts();
    let mut app = load(&library).expect("the guest loads");
    let mut tree = app.mount();
    assert_eq!(
        tree.on_event(&release()),
        EventResult::Handled,
        "precondition: the guest leaves a worker it cannot stop"
    );
    assert_eq!(app.take_panic(), None, "precondition: nothing panicked");
    drop(tree);
    drop(app);
    assert_eq!(mapped_guests(), 1, "its library stays mapped");
    KEPT_MAPPED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    let mut next = load(&library).expect("the next build loads");
    assert!(
        runs_a_frame(&mut next),
        "the host and the next build run on"
    );
}

/// Loads the guest, mounts and lays out its tree, hands it an event it handles, runs a frame and paints it, then drops both the way the runner does on a reload.
#[cfg(target_os = "linux")]
fn run_a_session(library: &Path) {
    load_the_host_fonts();
    let mut app = load(library).expect("the guest loads");
    app.set_surface_size(Size::new(320.0, 200.0));
    let mut tree = app.mount();
    app.relayout();
    assert_eq!(app.take_panic(), None, "precondition: the tree mounts");
    let mut dispatched = None;
    app.in_event_batch(&mut || {
        dispatched = Some((app.dispatch_overlays(&hover()), tree.on_event(&hover())));
    });
    let (overlaid, handled) = dispatched.expect("the dispatch ran");
    app.relayout();
    assert_eq!(app.take_panic(), None, "precondition: the move is handled");
    assert!(!overlaid, "precondition: no overlay takes the move");
    assert_eq!(
        handled,
        EventResult::Handled,
        "precondition: the tree takes the move"
    );
    assert!(runs_a_frame(&mut app), "precondition: its frame runs");
    assert!(!tree.frame().is_empty(), "precondition: the tree paints");
    tree.end_frame();
    assert_eq!(app.take_panic(), None, "precondition: nothing panicked");
    assert_eq!(mapped_guests(), 1, "precondition: it is mapped");
    drop(tree);
    drop(app);
}

/// What a host has done by the time it loads a guest, since its window built a renderer: loaded the faces it hands over, without which the guest has no measurer to lay its text out with.
fn load_the_host_fonts() {
    telar::font_families();
}

/// The bytes the process has allocated and not freed, in every arena: the guest's copy of std allocates from the same `malloc` as the host's.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn heap_in_use() -> usize {
    #[repr(C)]
    struct Mallinfo2 {
        arena: usize,
        ordblks: usize,
        smblks: usize,
        hblks: usize,
        hblkhd: usize,
        usmblks: usize,
        fsmblks: usize,
        uordblks: usize,
        fordblks: usize,
        keepcost: usize,
    }
    unsafe extern "C" {
        fn mallinfo2() -> Mallinfo2;
    }
    let info = unsafe { mallinfo2() };
    info.uordblks + info.hblkhd
}

/// Guests a test left mapped on purpose, for the rest of the process, which every other test's count leaves out.
#[cfg(target_os = "linux")]
static KEPT_MAPPED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[cfg(target_os = "linux")]
fn mapped_guests() -> usize {
    let maps = std::fs::read_to_string("/proc/self/maps").expect("read the process maps");
    let mapped = maps
        .lines()
        .filter(|line| line.contains("/.guest-"))
        .filter_map(|line| line.split_whitespace().nth(2))
        .filter(|start| *start == "00000000")
        .count();
    let kept = KEPT_MAPPED.load(std::sync::atomic::Ordering::Relaxed);
    assert!(
        mapped >= kept,
        "{kept} guests were kept mapped on purpose, but only {mapped} are mapped"
    );
    mapped - kept
}

/// Whether the guest's frame ran: it asks for a redraw every time it does.
fn runs_a_frame(app: &mut dyn AppRuntime) -> bool {
    let mut redraw_requested = false;
    let mut ctx = AppCtx::new(&mut redraw_requested, None, None, None);
    app.on_frame(&mut ctx);
    redraw_requested
}

fn hover() -> Event {
    Event::PointerMoved {
        x: 10.0,
        y: 10.0,
        source: PointerSource::Mouse,
    }
}

fn press() -> Event {
    Event::PointerPressed {
        x: 10.0,
        y: 10.0,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    }
}

fn secondary_press() -> Event {
    Event::PointerPressed {
        x: 10.0,
        y: 10.0,
        button: PointerButton::Secondary,
        source: PointerSource::Mouse,
    }
}

#[cfg(target_os = "linux")]
fn auxiliary_press() -> Event {
    Event::PointerPressed {
        x: 10.0,
        y: 10.0,
        button: PointerButton::Auxiliary,
        source: PointerSource::Mouse,
    }
}

#[cfg(target_os = "linux")]
fn release() -> Event {
    released(PointerButton::Primary)
}

#[cfg(target_os = "linux")]
fn secondary_release() -> Event {
    released(PointerButton::Secondary)
}

#[cfg(target_os = "linux")]
fn auxiliary_release() -> Event {
    released(PointerButton::Auxiliary)
}

#[cfg(target_os = "linux")]
fn released(button: PointerButton) -> Event {
    Event::PointerReleased {
        x: 10.0,
        y: 10.0,
        button,
        source: PointerSource::Mouse,
    }
}

fn panic_line() -> usize {
    FIXTURE_SOURCE
        .lines()
        .position(|line| line.trim_start().starts_with("panic!("))
        .expect("the fixture panics somewhere")
        + 1
}

fn build_fixture() -> PathBuf {
    guest_fixture::build_guest_fixture(Path::new(env!("CARGO_TARGET_TMPDIR")), "hot-panic-guest")
}

fn load(library: &Path) -> Result<HotApp, Box<dyn std::error::Error>> {
    // SAFETY: the fixture was just built from this checkout, against the telar this test links.
    unsafe { load_hot_app(library) }
}
