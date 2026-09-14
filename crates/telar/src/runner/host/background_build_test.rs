use std::sync::mpsc;
use std::time::Duration;

use super::*;

const PATIENCE: Duration = Duration::from_secs(5);

#[test]
fn a_build_still_running_hands_nothing_over() {
    let (finish, finished) = mpsc::channel::<()>();
    let (woke, woken) = mpsc::channel();
    let build = BackgroundBuild::spawn(
        move || {
            let _ = finished.recv();
            Ok(7)
        },
        move || {
            let _ = woke.send(());
        },
    );

    let Err(build) = build.try_take() else {
        panic!("a build that has not returned yet has nothing to hand over");
    };

    finish.send(()).unwrap();
    woken
        .recv_timeout(PATIENCE)
        .expect("the finished build wakes the loop");
    assert!(
        matches!(build.try_take(), Ok(Ok(7))),
        "the finished build hands its renderer over"
    );
}

#[test]
fn a_delivered_build_is_taken_before_its_thread_exits() {
    let (woke, woken) = mpsc::channel();
    let (release, released) = mpsc::channel::<()>();
    let build = BackgroundBuild::spawn(
        || Ok(7),
        move || {
            let _ = woke.send(());
            let _ = released.recv();
        },
    );

    woken
        .recv_timeout(PATIENCE)
        .expect("the builder wakes the loop");
    let taken = build.try_take();
    release.send(()).unwrap();

    assert!(
        matches!(taken, Ok(Ok(7))),
        "the frame the wake asked for must find the renderer while the builder is still inside the wake"
    );
}

#[test]
fn a_build_that_panics_fails_rather_than_building_forever() {
    let (woke, woken) = mpsc::channel();
    let build = BackgroundBuild::<u32>::spawn(
        || panic!("no adapter"),
        move || {
            let _ = woke.send(());
        },
    );

    woken
        .recv_timeout(PATIENCE)
        .expect("a panicked build still wakes the loop, or nothing would ever collect it");
    match build.try_take() {
        Ok(Err(RendererError::Backend(message))) => {
            assert_eq!(message, "renderer build thread panicked")
        }
        Ok(other) => panic!("a panicked build must fail, got {other:?}"),
        Err(_) => panic!("a panicked build must not read as still running"),
    }
}
