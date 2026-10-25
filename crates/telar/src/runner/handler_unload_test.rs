//! The end-to-end proof that a frame a hot-reloaded app drew never outlives its library: the guest `tests/fixtures/hot-panic-guest` paints text, which the host decodes into commands of its own, and the runner's own handler draws it, reloads it again and again and closes. A frame still held anywhere as its library is unmapped crashes the process as it lets go, so the session runs in a child process whose exit is the verdict. It compiles the fixture, so it is ignored by default: run it with `cargo test -p telar --features dev --lib unload -- --ignored`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use platform_headless::HeadlessWindow;

use super::*;
use crate::prefs::UserPrefs;
use crate::runner::host::SurfaceRenderer;

const CHILD: &str = "TELAR_UNLOAD_TEST_CHILD";
const RELOADS: usize = 4;

#[test]
#[ignore = "builds a cdylib against this checkout: run it with `cargo test -p telar --features dev --lib unload -- --ignored`"]
fn no_frame_a_hot_guest_drew_outlives_its_library() {
    let library = crate::guest_fixture::build_guest_fixture(&target_tmp(), "hot-panic-guest");
    if std::env::var_os(CHILD).is_some() {
        run_a_session(&library);
        return;
    }
    let name = concat!(
        module_path!(),
        "::no_frame_a_hot_guest_drew_outlives_its_library"
    );
    let name = name.split_once("::").map_or(name, |(_, path)| path);
    let output = Command::new(std::env::current_exe().expect("the test binary"))
        .args([name, "--exact", "--ignored", "--nocapture"])
        .env(CHILD, "1")
        .output()
        .expect("start the session");
    assert!(
        output.status.success(),
        "the host did not survive the session: {}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
}

fn run_a_session(library: &Path) {
    crate::font_families();
    let window = HeadlessWindow::new(320, 200);
    let mut handler = build_app_handler::<HeadlessWindow, ()>(
        load(library),
        Arc::new(services_core::NoPaths),
        crate::runner::font_config::FontSetup::default(),
        RendererBackend::Software,
        UserPrefs::default(),
        "unload-test".to_string(),
        SurfaceRenderer::builtin(),
    );
    handler.new_events();
    assert!(
        handler.on_resume(&window),
        "precondition: a renderer is built"
    );
    handler.about_to_wait();
    draw(&mut handler, &window);
    for _ in 0..RELOADS {
        handler.take_build(load(library), &window);
        draw(&mut handler, &window);
    }
    drop(handler);
}

fn load(library: &Path) -> Box<dyn AppRuntime> {
    // SAFETY: the fixture was just built from this checkout, against the telar this test links.
    Box::new(unsafe { crate::hot::load_hot_app(library) }.expect("the guest loads"))
}

fn draw(handler: &mut AppHandler<HeadlessWindow, ()>, window: &HeadlessWindow) {
    handler.new_events();
    std::thread::sleep(super::super::FRAME_BUDGET);
    handler.on_redraw(window);
    handler.about_to_wait();
    assert!(
        handler.last_frame_rgba().is_some(),
        "precondition: the frame is drawn"
    );
}

/// Where cargo keeps what tests build for themselves: `target/tmp`, which it names only to integration tests. A unit test binary sits in `target/<profile>/deps`.
fn target_tmp() -> PathBuf {
    let exe = std::env::current_exe().expect("the test binary");
    exe.ancestors()
        .nth(3)
        .expect("the binary is under the target directory")
        .join("tmp")
}
