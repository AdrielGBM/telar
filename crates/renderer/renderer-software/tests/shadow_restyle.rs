//! A text shadow big enough to be blurred off-thread must not blink out when the string changes.
//!
//! The shadow cache is keyed on the text, so a clock re-keys its shadow every minute. Past
//! `ASYNC_SHADOW_THRESHOLD` the blur moves to a worker and the result lands a frame or two later — and the
//! frames in between used to draw no shadow at all, which on a desktop clock reads as the shadow blinking
//! once a minute.

use std::sync::Arc;

use geometry_core::Rect;
use platform_headless::HeadlessWindow;
use renderer_core::{Color, DrawCommand, RenderBackend, Shadow, TextStyle};
use telar_renderer_software::{SoftwareRenderer, SoftwareRendererConfig};

const W: u32 = 640;
const H: u32 = 320;

/// Big enough that the shadow pixmap clears `ASYNC_SHADOW_THRESHOLD` (80 000 px) and takes the worker path —
/// the only path this test is about.
fn clock_like(text: &str) -> Vec<DrawCommand> {
    vec![DrawCommand::Text {
        spans: None,
        text: Arc::from(text),
        rect: Rect::new(20.0, 20.0, 600.0, 200.0),
        style: Arc::new(
            TextStyle::new(140.0, Color::WHITE).with_text_shadow(Shadow {
                color: Color::from_rgb_u8(255, 0, 0),
                blur_radius: 12.0,
                offset_x: 0.0,
                offset_y: 0.0,
                spread: 0.0,
            }),
        ),
    }]
}

/// How much red the frame carries. The shadow is the only red thing drawn, so this is "is the shadow there".
fn shadow_presence(rgba: &[u8]) -> u64 {
    rgba.chunks_exact(4)
        .map(|px| u64::from(px[0].saturating_sub(px[2])))
        .sum()
}

fn draw(renderer: &mut SoftwareRenderer<HeadlessWindow, HeadlessWindow>, text: &str) -> u64 {
    renderer.begin_frame(W, H, 1.0, 0).unwrap();
    renderer
        .render_frame(&clock_like(text), Some(Color::BLACK))
        .unwrap();
    shadow_presence(&renderer.read_rgba().expect("pixmap after a frame"))
}

#[test]
fn a_rekeyed_text_shadow_keeps_drawing_while_its_blur_is_in_flight() {
    let mut renderer = SoftwareRenderer::<HeadlessWindow, HeadlessWindow>::new_headless(
        W,
        H,
        SoftwareRendererConfig::default(),
    );

    // Settle the first shadow: the worker needs a few frames to land its result in the cache.
    let mut settled = 0;
    for _ in 0..200 {
        settled = draw(&mut renderer, "15:47");
        if settled > 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(
        settled > 0,
        "the async shadow never landed, so this test proves nothing"
    );

    // The minute rolls over: new text, new cache key, blur back on a worker. This is the frame that used to
    // come out with no shadow at all.
    let during = draw(&mut renderer, "15:48");

    assert!(
        during > settled / 2,
        "shadow vanished while its blur was in flight: {during} vs {settled} settled"
    );
}
