//! The end-to-end proof that a panic in a hot-reloaded app stops at the boundary: `tests/fixtures/hot-panic-guest`, a cdylib carrying its own copy of std whose press handler panics, is loaded with `load_hot_app` and pressed, and the host must get back what panicked and where, never call into that guest again, load the next build, and still be running to say so; and that a guest dropped after it mounted, drove and painted a tree is unmapped, so reload after reload leaves one build mapped. It compiles the fixture against this checkout into its own target directory under `target/tmp`, so it is ignored by default: run it with `cargo test -p telar --features dev --test hot_panic -- --ignored`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, PoisonError};

use telar::hot::load_hot_app;
use telar::{AppCtx, AppRuntime, Event, EventResult, PointerButton, PointerSource, Size};

/// One at a time: both count what the process has mapped and what sits in the build directory.
static SERIAL: Mutex<()> = Mutex::new(());

const FIXTURE_SOURCE: &str = include_str!("../tests/fixtures/hot-panic-guest/src/lib.rs");

#[test]
#[ignore = "builds a cdylib against this checkout: run it with `cargo test -p telar --features dev --test hot_panic -- --ignored`"]
fn a_panic_in_a_hot_guest_is_reported_and_the_next_build_takes_over() {
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    let library = build_fixture();
    load_the_host_fonts();

    let mut broken = load_hot_app(&library).expect("the guest loads");
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

    let mut next =
        load_hot_app(&library).expect("the next build loads beside the one still mapped");
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
    let mut app = load_hot_app(&library).expect("the guest loads");
    assert!(runs_a_frame(&mut app), "precondition: its frame runs");
    assert_eq!(mapped_guests(), 1, "precondition: it is mapped");
    drop(app);
    assert_eq!(
        mapped_guests(),
        0,
        "calling in through `guest_call` leaves nothing that pins the library"
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

#[test]
#[ignore = "builds a cdylib against this checkout: run it with `cargo test -p telar --features dev --test hot_panic -- --ignored`"]
#[cfg(target_os = "linux")]
fn reloading_a_hot_guest_again_and_again_keeps_one_mapped() {
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    let library = build_fixture();
    for _ in 0..5 {
        run_a_session(&library);
    }
    let mut last = load_hot_app(&library).expect("the guest loads");
    assert!(runs_a_frame(&mut last), "precondition: its frame runs");
    assert_eq!(
        mapped_guests(),
        1,
        "only the build in use is mapped, however many came before it"
    );
}

/// Loads the guest, mounts and lays out its tree, hands it an event it handles, runs a frame and paints it, then drops both the way the runner does on a reload.
#[cfg(target_os = "linux")]
fn run_a_session(library: &Path) {
    load_the_host_fonts();
    let mut app = load_hot_app(library).expect("the guest loads");
    app.set_surface_size(Size::new(320.0, 200.0));
    let mut tree = app.mount();
    app.relayout();
    assert_eq!(app.take_panic(), None, "precondition: the tree mounts");
    app.begin_event_batch();
    let overlaid = app.dispatch_overlays(&hover());
    let handled = tree.on_event(&hover());
    app.end_event_batch();
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

#[cfg(target_os = "linux")]
fn mapped_guests() -> usize {
    let maps = std::fs::read_to_string("/proc/self/maps").expect("read the process maps");
    maps.lines()
        .filter(|line| line.contains("/.guest-"))
        .filter_map(|line| line.split_whitespace().nth(2))
        .filter(|start| *start == "00000000")
        .count()
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

fn panic_line() -> usize {
    FIXTURE_SOURCE
        .lines()
        .position(|line| line.trim_start().starts_with("panic!("))
        .expect("the fixture panics somewhere")
        + 1
}

/// Builds the fixture into a target directory of its own: the `cargo test` running this holds the workspace's until it returns.
fn build_fixture() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixture = manifest_dir.join("tests/fixtures/hot-panic-guest");
    let target_dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("hot-panic");
    // Offline, the versions the workspace already fetched are the only ones the fixture can resolve to, and the same ones let it reuse what earlier runs compiled.
    std::fs::copy(
        manifest_dir.join("../../Cargo.lock"),
        fixture.join("Cargo.lock"),
    )
    .expect("copy Cargo.lock");
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(cargo)
        .args(["build", "--lib", "--manifest-path"])
        .arg(fixture.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", &target_dir)
        .output()
        .expect("start cargo");
    assert!(
        output.status.success(),
        "the fixture did not build: {}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    target_dir.join("debug").join(format!(
        "{}hot_panic_guest{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    ))
}
