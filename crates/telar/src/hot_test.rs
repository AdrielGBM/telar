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

thread_local! {
    static HANDED: std::cell::Cell<Option<platform_core::KeyboardConvention>> = const { std::cell::Cell::new(None) };
    static HANDED_BEFORE_BUILD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static FONTS_HANDED: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

struct Blank;

impl App for Blank {
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

fn hand_over(services: HostServices) -> GuestResult<()> {
    HANDED.set(Some(services.keyboard_convention));
    FONTS_HANDED.set(Some(services.fonts.is_some()));
    Ok(())
}

fn build_blank() -> GuestResult<Box<dyn App>> {
    HANDED_BEFORE_BUILD.set(HANDED.get().is_some());
    guest::create_app(|| Box::new(Blank))
}

fn foreign_convention() -> platform_core::KeyboardConvention {
    match platform_core::KeyboardConvention::native() {
        platform_core::KeyboardConvention::Mac => platform_core::KeyboardConvention::Pc,
        platform_core::KeyboardConvention::Pc => platform_core::KeyboardConvention::Mac,
    }
}

/// A build carries its own copy of the convention, which left alone answers for the system it was compiled on: a terminal host on a Mac matches Ctrl while the build would show ⌘.
#[test]
fn a_build_is_handed_the_host_keyboard_convention_before_it_is_made() {
    let host = foreign_convention();
    platform_core::set_keyboard_convention(host);
    let exports = GuestExports {
        install_services: hand_over,
        ..GuestExports::in_process()
    };
    let app = HotApp::start(exports, build_blank, None);
    assert_eq!(HANDED.get(), Some(host));
    assert_eq!(
        FONTS_HANDED.get(),
        Some(false),
        "exports this process serves itself already set text in its one font database, which handed over would defer to itself"
    );
    assert!(
        HANDED_BEFORE_BUILD.get(),
        "the app's setup already shows shortcuts the host's way"
    );
    drop(app);
}

#[test]
fn installing_the_host_services_installs_its_keyboard_convention() {
    let host = foreign_convention();
    platform_core::set_keyboard_convention(host);
    let services = HostServices::installed().in_this_process();
    platform_core::set_keyboard_convention(platform_core::KeyboardConvention::native());
    services.install();
    assert_eq!(platform_core::keyboard_convention(), host);
}

thread_local! {
    static CALLS: std::cell::RefCell<Vec<&'static str>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn record(call: &'static str) -> GuestResult<()> {
    CALLS.with_borrow_mut(|calls| calls.push(call));
    Ok(())
}

fn keep_services(_: HostServices) -> GuestResult<()> {
    Ok(())
}

fn release_app(app: Box<dyn App>) -> GuestResult<()> {
    drop(app);
    record("app_release")
}

fn mount_tree(_: &dyn App) -> GuestResult<*mut HotTree> {
    record("tree_mount")?;
    Ok(std::ptr::NonNull::dangling().as_ptr())
}

fn paint_tree(_: *mut HotTree) -> GuestResult<Vec<renderer_core::DrawCommand>> {
    record("tree_paint")?;
    Ok(Vec::new())
}

fn release_tree(_: *mut HotTree) -> GuestResult<()> {
    record("tree_release")
}

/// A tree handed out by `mount` is `'static`, so safe code can keep it past the app it came from: it keeps driving its guest, and is released into it, rather than calling code the app took with it.
#[test]
fn a_tree_outliving_its_app_still_drives_its_guest() {
    use crate::app_runtime::AppRuntime;
    CALLS.with_borrow_mut(Vec::clear);
    let exports = GuestExports {
        install_services: keep_services,
        app_release: release_app,
        tree_mount: mount_tree,
        tree_paint: paint_tree,
        tree_release: release_tree,
        ..GuestExports::in_process()
    };
    let mut app = HotApp::start(exports, build_blank, None);
    let tree = app.mount();
    drop(app);
    let _ = tree.frame();
    drop(tree);
    assert_eq!(
        CALLS.take(),
        ["tree_mount", "app_release", "tree_paint", "tree_release"]
    );
}

#[cfg(unix)]
fn build_panicking() -> GuestResult<Box<dyn App>> {
    guest::create_app(|| panic!("the build could not find its assets"))
}

/// Held with this process as its library, so dropping it runs the unload path a built library's does.
#[cfg(unix)]
fn start_as_loaded(create: CreateApp) -> HotApp {
    let exports = GuestExports {
        install_services: keep_services,
        app_release: release_app,
        loaded: || record("loaded"),
        unloading: || record("unloading"),
        quiesce: || record("quiesce"),
        ..GuestExports::in_process()
    };
    let this_process = libloading::Library::from(libloading::os::unix::Library::this());
    HotApp::start(exports, create, Some(this_process))
}

#[cfg(unix)]
#[test]
fn a_guest_is_released_then_quiesced_before_its_library_is_closed() {
    CALLS.with_borrow_mut(Vec::clear);
    drop(start_as_loaded(build_blank));
    assert_eq!(
        CALLS.take(),
        ["loaded", "app_release", "unloading", "quiesce"]
    );
}

/// Its worker threads may still be running its code, so the one call a guest that panicked still gets is the one that joins them.
#[cfg(unix)]
#[test]
fn a_guest_that_panicked_is_still_quiesced_before_its_library_is_closed() {
    CALLS.with_borrow_mut(Vec::clear);
    let app = start_as_loaded(build_panicking);
    assert!(app.image.guest.has_panicked(), "precondition: it panicked");
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
