//! Integration test for the headless `Platform` backend (Sprint B): drive a real rsx app end-to-end through
//! `run_with_platform` + `HeadlessPlatform` and assert on the read-back pixels. This exercises the full
//! pipeline (event → reactive → layout → software render → pixels) through the *same* `EventHandler` seam the
//! winit backend uses — the headless backend only swaps the window/loop.

mod common;

use std::sync::{Arc, Mutex};

use common::{FillApp, NullPaths, assert_center_rgb};
use platform_headless::{FrameSink, HeadlessPlatform};
use telar::{AppConfig, AppPathsProvider, Color, run_with_platform};

#[test]
fn headless_renders_fill_color_to_pixels() {
    let (w, h) = (64u32, 48u32);
    let sink: FrameSink = Arc::new(Mutex::new(None));

    let platform = HeadlessPlatform::new(w, h)
        .with_frames(2)
        .capture_into(sink.clone());

    // `()` is the no-op dev plugin — the same one a non-dev desktop build uses.
    run_with_platform::<_, _, ()>(
        platform,
        AppConfig::default(),
        Box::new(NullPaths) as Box<dyn AppPathsProvider>,
        FillApp {
            color: Color::from_rgb_u8(50, 120, 200),
        },
        "telar-headless-test",
    )
    .expect("headless run failed");

    let pixels = sink.lock().unwrap().take().expect("no frame was captured");
    assert_center_rgb(&pixels, w, h, [50, 120, 200], "single-surface fill");
}
