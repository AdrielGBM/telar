//! The split-runtime regression test: with the app loaded as a real dylib (host and app linking separate
//! reactive/layout/motion copies), does a click alone keep re-composing frames?
//!
//! This is the condition no in-process test can reproduce, and the one that made an entrance transition sit at
//! opacity 0 — a black page — until the user moved the mouse. It needs a dylib built the way `cargo telar dev`
//! builds one, so it is `#[ignore]`d by default. To run it:
//!
//! ```text
//! TELAR_HOT_RELOAD_BUILD=1 RUSTFLAGS=--cfg=telar_hot_reload cargo build -p sandbox --features sandbox/dev --lib
//! cargo test -p sandbox --features dev --test hot_tree -- --ignored
//! ```

#![cfg(feature = "dev")]

use std::path::PathBuf;
use std::time::Duration;

use platform_core::{Event, EventHandler, PointerButton, PointerSource};
use platform_headless::HeadlessWindow;
use telar::{App, AppPathsProvider, NoPaths, build_surface_handler};

fn dylib_path() -> PathBuf {
    let name = if cfg!(target_os = "windows") {
        "sandbox.dll"
    } else if cfg!(target_os = "macos") {
        "libsandbox.dylib"
    } else {
        "libsandbox.so"
    };
    // CARGO_MANIFEST_DIR is apps/sandbox; the workspace target dir is two levels up.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/debug")
        .join(name)
}

/// The reload ownership order: the tree the dylib allocated has to be released *into that dylib* while it is
/// still mapped, which is why the runner nulls its tree before it replaces the app. Getting it backwards frees
/// dylib memory through an unmapped `release` shim — a segfault, not a test failure, so this is a crash guard.
#[test]
#[ignore = "needs a hot-reload dylib built first; see the module docs"]
fn the_dylib_tree_composes_and_is_released_before_its_library() {
    let path = dylib_path();
    assert!(path.exists(), "no dylib at {}", path.display());
    let mut app = telar::hot::load_hot_app(&path).expect("dlopen failed");
    let mut tree = app.mount();
    tree.on_event(&Event::WindowResized {
        width: 1200,
        height: 900,
    });
    assert!(
        !tree.frame().is_empty(),
        "the dylib's tree composed no draw commands"
    );
    assert!(tree.generation() > 0);
    // The runner's order, and the reason `HotTreeHandle` exists at all.
    drop(tree);
    drop(app);
}

#[test]
#[ignore = "needs a hot-reload dylib built first; see the module docs"]
fn a_click_keeps_recomposing_frames_with_the_app_in_a_dylib() {
    let path = dylib_path();
    assert!(
        path.exists(),
        "no dylib at {}: build it as the module docs describe",
        path.display()
    );
    let app = telar::hot::load_hot_app(&path).expect("dlopen failed");
    let (w, h) = (1200u32, 900u32);
    let window = HeadlessWindow::new(w, h);
    let mut handler: Box<dyn EventHandler<HeadlessWindow>> = build_surface_handler(
        app,
        std::sync::Arc::new(NoPaths) as std::sync::Arc<dyn AppPathsProvider>,
        "telar-sandbox-hot-tree-test",
    );
    handler.new_events();
    assert!(handler.on_resume(&window), "headless renderer init failed");
    handler.about_to_wait();
    for event in [Event::WindowResized {
        width: w,
        height: h,
    }] {
        handler.new_events();
        handler.on_event(event, &window);
        handler.about_to_wait();
    }
    handler.new_events();
    handler.on_redraw(&window);
    handler.about_to_wait();

    // Click nav item 5 in the rail, then stop touching the input entirely.
    let (x, y) = (110.0, 489.0);
    for event in [
        Event::PointerPressed {
            x,
            y,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        },
        Event::PointerReleased {
            x,
            y,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        },
    ] {
        handler.new_events();
        handler.on_event(event, &window);
        handler.about_to_wait();
    }

    // Frames with no input at all: the transition must keep producing *different* pixels. Frozen pixels are the
    // bug — the host re-sending the commands composed for the animation's first value.
    let mut distinct = 0;
    let mut previous: Option<Vec<u8>> = None;
    for _ in 0..12 {
        handler.new_events();
        handler.on_redraw(&window);
        let paced = handler.about_to_wait();
        if let Some(pixels) = handler.last_frame_rgba() {
            if previous.as_ref() != Some(&pixels) {
                distinct += 1;
            }
            previous = Some(pixels);
        }
        if paced.is_none() {
            break;
        }
        std::thread::sleep(Duration::from_millis(4));
    }
    assert!(
        distinct >= 2,
        "only {distinct} distinct frame(s) after the click with no further input: the app's tree is not \
         re-composing on its own. If the dylib was last built without TELAR_HOT_RELOAD_BUILD it exports no \
         tree shims and the host mounted the tree itself — rebuild it as the module docs describe."
    );
}
