use super::*;

#[cfg(feature = "desktop-bare")]
#[test]
fn each_line_of_the_channel_is_the_event_its_prefix_names() {
    assert_eq!(
        hot_event("hot:/target/debug/libapp.so"),
        Some(HotEvent::Reload("/target/debug/libapp.so".into()))
    );
    assert_eq!(
        hot_event("err:line\\none"),
        Some(HotEvent::BuildError("line\none".to_string()))
    );
    assert_eq!(
        hot_event("goto:/preview/demo--card--a?args=label:%22Hi%22"),
        Some(HotEvent::Goto(
            "/preview/demo--card--a?args=label:%22Hi%22".to_string()
        ))
    );
    assert_eq!(hot_event("focus"), Some(HotEvent::Focus));
    assert_eq!(hot_event("/target/debug/libapp.so"), None);
}

use platform_core::guest::{Lifecycle, Timestamp};

thread_local! {
    static HANDED: std::cell::Cell<Option<platform_core::KeyboardConvention>> = const { std::cell::Cell::new(None) };
    static HANDED_BEFORE_BUILD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static SERVED_NOTHING: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
    static CALLS: std::cell::RefCell<Vec<&'static str>> = const { std::cell::RefCell::new(Vec::new()) };
}

struct Blank;

impl crate::app::App for Blank {
    fn root(&self) -> Box<dyn ui_tree::Component> {
        Box::new(BlankView)
    }
}

struct BlankView;

impl ui_tree::Component for BlankView {
    fn view(&self) -> ui_tree::RenderNode {
        ui_tree::RenderNode::group([])
    }
}

unsafe extern "C" fn hand_over(services: HostServices) -> CallOutcome {
    HANDED.set(unsafe { services.ask(services.keyboard_convention, &()) });
    SERVED_NOTHING.set(Some(services.data.is_null()));
    CallOutcome::OK
}

unsafe extern "C" fn build_blank(app: *mut Handle<GuestApp>) -> CallOutcome {
    HANDED_BEFORE_BUILD.set(HANDED.get().is_some());
    unsafe { guest::create(app, || Box::new(Blank)) }
}

fn foreign_convention() -> platform_core::KeyboardConvention {
    match platform_core::KeyboardConvention::native() {
        platform_core::KeyboardConvention::Mac => platform_core::KeyboardConvention::Pc,
        platform_core::KeyboardConvention::Pc => platform_core::KeyboardConvention::Mac,
    }
}

fn record(call: &'static str) -> CallOutcome {
    CALLS.with_borrow_mut(|calls| calls.push(call));
    CallOutcome::OK
}

extern "C" fn record_loaded() -> CallOutcome {
    record("loaded")
}

extern "C" fn record_unloading() -> CallOutcome {
    record("unloading")
}

extern "C" fn record_quiesce() -> CallOutcome {
    record("quiesce")
}

extern "C" fn keep_services(_: HostServices) -> CallOutcome {
    CallOutcome::OK
}

/// The library's lifecycle with `install_services`, recording the calls that would make this test process a guest rather than running them.
fn recorded(install_services: unsafe extern "C" fn(HostServices) -> CallOutcome) -> Lifecycle {
    Lifecycle {
        loaded: record_loaded,
        unloading: record_unloading,
        quiesce: record_quiesce,
        ..Lifecycle::serving(install_services)
    }
}

/// A build carries its own copy of the convention, which left alone answers for the system it was compiled on: a terminal host on a Mac matches Ctrl while the build would show ⌘.
#[cfg(unix)]
#[test]
fn a_build_is_handed_the_host_services_before_it_is_made() {
    let host = foreign_convention();
    platform_core::set_keyboard_convention(host);
    let entries = HotEntries {
        lifecycle: recorded(hand_over),
        ..HotEntries::serving(build_blank)
    };
    let this_process = libloading::Library::from(libloading::os::unix::Library::this());
    let app = HotApp::start(entries, Some(this_process));
    assert_eq!(HANDED.get(), Some(host));
    assert!(
        HANDED_BEFORE_BUILD.get(),
        "the app's setup already shows shortcuts the host's way"
    );
    drop(app);
}

#[test]
fn entries_this_process_serves_itself_are_served_nothing() {
    let entries = HotEntries {
        lifecycle: Lifecycle::serving(hand_over),
        ..HotEntries::serving(build_blank)
    };
    let app = HotApp::start(entries, None);
    assert_eq!(
        SERVED_NOTHING.get(),
        Some(true),
        "they share every store of this process already, its one font database included, which handed over would defer to itself"
    );
    drop(app);
}

unsafe extern "C" fn release_app(app: Handle<GuestApp>) -> CallOutcome {
    unsafe { app.release() };
    record("release")
}

unsafe extern "C" fn mount_tree(app: Handle<GuestApp>, tree: *mut Handle<HotTree>) -> CallOutcome {
    unsafe { guest::mount(app, tree) };
    record("mount")
}

unsafe extern "C" fn paint_tree(tree: Handle<HotTree>, frame: *mut LentBytes) -> CallOutcome {
    unsafe { guest::tree_paint(tree, frame) };
    record("tree_paint")
}

unsafe extern "C" fn release_tree(tree: Handle<HotTree>) -> CallOutcome {
    unsafe { tree.release() };
    record("tree_release")
}

/// A tree handed out by `mount` is `'static`, so safe code can keep it past the app it came from: it keeps driving its guest, and is released into it, rather than calling code the app took with it.
#[test]
fn a_tree_outliving_its_app_still_drives_its_guest() {
    use crate::app_runtime::AppRuntime;
    CALLS.with_borrow_mut(Vec::clear);
    let entries = HotEntries {
        lifecycle: Lifecycle::serving(keep_services),
        release: release_app,
        mount: mount_tree,
        tree_paint: paint_tree,
        tree_release: release_tree,
        ..HotEntries::serving(build_blank)
    };
    let mut app = HotApp::start(entries, None);
    let tree = app.mount();
    drop(app);
    let _ = tree.frame();
    drop(tree);
    assert_eq!(
        CALLS.take(),
        ["mount", "release", "tree_paint", "tree_release"]
    );
}

thread_local! {
    static SKIPPED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

unsafe extern "C" fn paint_skipping_one(
    tree: Handle<HotTree>,
    frame: *mut LentBytes,
) -> CallOutcome {
    let painted = unsafe { tree.get() };
    if !SKIPPED.replace(true) {
        painted.paint();
    }
    unsafe { frame.write(painted.paint()) };
    record("tree_paint")
}

unsafe extern "C" fn reset_frames(tree: Handle<HotTree>) -> CallOutcome {
    unsafe { guest::tree_reset_frames(tree) };
    record("tree_reset_frames")
}

/// Frames build on the ones before them, so one the host never read leaves its decoder a step behind: the guest is asked for a frame that stands alone, and the host still draws what the tree composed.
#[test]
fn a_frame_the_host_cannot_read_is_painted_again_on_its_own() {
    use crate::app_runtime::AppRuntime;
    CALLS.with_borrow_mut(Vec::clear);
    let entries = HotEntries {
        lifecycle: Lifecycle::serving(keep_services),
        tree_paint: paint_skipping_one,
        tree_reset_frames: reset_frames,
        ..HotEntries::serving(build_blank)
    };
    let mut app = HotApp::start(entries, None);
    let tree = app.mount();
    let _ = tree.frame();
    assert_eq!(
        CALLS.take(),
        ["tree_paint", "tree_reset_frames", "tree_paint"]
    );
    assert_eq!(app.take_panic(), None);
}

unsafe extern "C" fn tick_at(now: Timestamp) -> CallOutcome {
    TICKED.set(Some(now));
    CallOutcome::OK
}

thread_local! {
    static TICKED: std::cell::Cell<Option<Timestamp>> = const { std::cell::Cell::new(None) };
}

/// An `Instant` means nothing to another copy of std, so a tick crosses as the time since the epoch the guest adopted when it loaded.
#[test]
fn a_tick_crosses_as_the_time_since_the_epoch_the_guest_adopted() {
    use crate::app_runtime::AppRuntime;
    let entries = HotEntries {
        lifecycle: Lifecycle::serving(keep_services),
        motion_tick: tick_at,
        ..HotEntries::serving(build_blank)
    };
    let app = HotApp::start(entries, None);
    let later = app.library().epoch().instant(Timestamp { nanos: 5_000 });
    app.motion_tick(later);
    assert_eq!(TICKED.get(), Some(Timestamp { nanos: 5_000 }));
}

/// What the guest answers in bytes reads back on the host's side as the values it spelled.
#[test]
fn what_a_guest_lends_reads_back_as_what_it_spelled() {
    use crate::app_runtime::AppRuntime;
    let entries = HotEntries {
        lifecycle: Lifecycle::serving(keep_services),
        ..HotEntries::serving(build_blank)
    };
    let mut app = HotApp::start(entries, None);
    assert!(app.window_config().is_none());
    assert_eq!(app.text_input(), None);
    assert_eq!(app.drain_window_commands(), Vec::new());
    let tree = app.mount();
    let mut nodes = Vec::new();
    tree.walk(&mut nodes);
    assert!(!nodes.is_empty(), "the tree walks");
    assert!(app.hot_snapshot().is_some());
    assert_eq!(app.take_panic(), None);
}

unsafe extern "C" fn build_panicking(app: *mut Handle<GuestApp>) -> CallOutcome {
    unsafe { guest::create(app, || panic!("the build could not find its assets")) }
}

/// Held with this process as its library, so dropping it runs the unload path a built library's does.
#[cfg(unix)]
fn start_as_loaded(create: CreateApp) -> HotApp {
    let entries = HotEntries {
        lifecycle: recorded(keep_services),
        release: release_app,
        ..HotEntries::serving(create)
    };
    let this_process = libloading::Library::from(libloading::os::unix::Library::this());
    HotApp::start(entries, Some(this_process))
}

#[cfg(unix)]
#[test]
fn a_guest_is_released_then_quiesced_before_its_library_is_closed() {
    CALLS.with_borrow_mut(Vec::clear);
    drop(start_as_loaded(build_blank));
    assert_eq!(CALLS.take(), ["loaded", "release", "unloading", "quiesce"]);
}

/// Its worker threads may still be running its code, so the one call a guest that panicked still gets is the one that joins them.
#[cfg(unix)]
#[test]
fn a_guest_that_panicked_is_still_quiesced_before_its_library_is_closed() {
    CALLS.with_borrow_mut(Vec::clear);
    let app = start_as_loaded(build_panicking);
    assert!(
        app.library().guest().has_panicked(),
        "precondition: it panicked"
    );
    let report = app.library().guest().panic().expect("the panic is kept");
    assert_eq!(report.message, "the build could not find its assets");
    assert!(report.location.is_some(), "it says where");
    drop(app);
    assert_eq!(CALLS.take(), ["loaded", "quiesce"]);
}

#[cfg(feature = "desktop-bare")]
#[test]
fn the_app_opens_the_hot_channel_with_its_token() {
    use std::io::BufRead;
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let _app = connect_hot_channel(port, "s3cret").unwrap();
    let (cli, _) = listener.accept().unwrap();
    let mut first = String::new();
    std::io::BufReader::new(cli).read_line(&mut first).unwrap();
    assert_eq!(first, "s3cret\n");
}

#[cfg(feature = "desktop-bare")]
#[test]
fn the_app_says_it_answers_reloads_then_answers_each_one_naming_its_library() {
    use std::io::BufRead;
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let app = connect_hot_channel(port, "s3cret").unwrap();
    let (replies_tx, replies) = std::sync::mpsc::channel();
    answer_reloads(app, replies);
    let sent = [
        HotOutcome::Refused("libapp.so: line one\nline two".to_owned()),
        HotOutcome::Panicked("panicked at src/app.rs:4:9:\nno assets".to_owned()),
        HotOutcome::Loaded,
    ]
    .map(|outcome| HotReply {
        library: "/t/libapp.so".to_owned(),
        outcome,
    });
    let replies = HotReplies::new(replies_tx);
    for reply in sent.clone() {
        replies.send(reply);
    }

    let (cli, _) = listener.accept().unwrap();
    cli.set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .unwrap();
    let lines: Vec<String> = std::io::BufReader::new(cli)
        .lines()
        .take(5)
        .map(Result::unwrap)
        .collect();
    assert_eq!(lines[..2], ["s3cret", HOT_REPLIES_LINE]);
    let heard: Vec<_> = lines[2..]
        .iter()
        .map(|line| HotReply::parse(line))
        .collect();
    assert_eq!(heard, sent.map(Some));
}
