//! Does a click alone keep frames coming? A navigation starts a `NavTransition::Fade`, and the loop must keep
//! scheduling frames for as long as it is unsettled — without the user moving the mouse to generate events.
//! Driven through the same `EventHandler` seam winit uses, so a regression here is a regression on the desktop.
//!
//! Asserted as a contract, not as a frame count: how many frames a 220ms fade gets depends on what a frame
//! costs (in a debug build with the software renderer, a section swap costs tens of ms), so the invariant is
//! "the loop asks to be woken exactly while something is animating".

use std::time::Duration;

use platform_core::{Event, EventHandler, PointerButton, PointerSource};
use platform_headless::HeadlessWindow;
use telar::{AppPathsProvider, NoPaths, build_surface_handler};

/// One loop iteration that delivers an event, returning the pace the handler asks to be woken at.
fn feed(
    handler: &mut Box<dyn EventHandler<HeadlessWindow>>,
    window: &HeadlessWindow,
    event: Event,
) -> Option<Duration> {
    handler.new_events();
    handler.on_event(event, window);
    handler.about_to_wait()
}

/// One loop iteration that renders, returning the pace the handler asks to be woken at. `None` means "nothing
/// to do, sleep until real input" — which while an animation runs is the symptom of a stalled transition.
fn frame(
    handler: &mut Box<dyn EventHandler<HeadlessWindow>>,
    window: &HeadlessWindow,
) -> Option<Duration> {
    handler.new_events();
    handler.on_redraw(window);
    handler.about_to_wait()
}

fn press(x: f64, y: f64) -> Event {
    Event::PointerPressed {
        x,
        y,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    }
}

fn release(x: f64, y: f64) -> Event {
    Event::PointerReleased {
        x,
        y,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    }
}

#[test]
fn a_nav_click_keeps_the_loop_scheduling_frames_until_the_fade_settles() {
    let (w, h) = (1200u32, 900u32);
    // The `app!` setup closure normally does this before the tree mounts.
    telar::set_theme(sandbox::core::theme::SandboxTheme::modern());
    let window = HeadlessWindow::new(w, h);
    let mut handler: Box<dyn EventHandler<HeadlessWindow>> = build_surface_handler(
        sandbox::core::app::SandboxRoot,
        std::sync::Arc::new(NoPaths) as std::sync::Arc<dyn AppPathsProvider>,
        "telar-sandbox-pacing-test",
    );

    handler.new_events();
    assert!(handler.on_resume(&window), "headless renderer init failed");
    handler.about_to_wait();
    feed(
        &mut handler,
        &window,
        Event::WindowResized {
            width: w,
            height: h,
        },
    );
    frame(&mut handler, &window);
    frame(&mut handler, &window);

    // Nav item 5 in the rail, at the coordinates the other shell tests use. The press only arms the button;
    // `on_press` fires on release, which is where the navigation (and its fade) starts.
    let (x, y) = (110.0, 489.0);
    feed(&mut handler, &window, press(x, y));
    let after_release = feed(&mut handler, &window, release(x, y));
    assert!(
        telar::motion::has_active(),
        "the click did not start the page transition"
    );
    assert!(
        after_release.is_some(),
        "a click that started a transition left the loop with nothing to wake up for: the fade would only \
         advance when some unrelated event arrived"
    );

    // Drive frames with NO further input, as the runner's own timer wake does. While the fade is unsettled the
    // handler must keep asking for the next frame; once it settles it must stop, so the loop can sleep.
    for i in 0..60 {
        let pace = frame(&mut handler, &window);
        if telar::motion::has_active() {
            assert!(
                pace.is_some(),
                "frame {i}: the fade is still running but the loop was told it could sleep"
            );
        } else {
            assert!(
                pace.is_none(),
                "frame {i}: nothing is animating, yet the loop keeps scheduling frames"
            );
            return;
        }
        std::thread::sleep(Duration::from_millis(4));
    }
    panic!("the transition never settled — the loop would keep spinning");
}
