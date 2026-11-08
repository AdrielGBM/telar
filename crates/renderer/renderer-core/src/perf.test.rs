use super::*;

#[test]
fn a_capture_records_its_own_thread_and_nothing_else() {
    let ((), snapshot) = capture(|| {
        std::thread::spawn(|| {
            drop(span(Phase::Plan));
            note_damage(true);
            note_damage_area(1, 1);
        })
        .join()
        .unwrap();
        drop(span(Phase::Frame));
        note_damage(true);
        note_damage_area(25, 100);
    });

    assert_eq!(snapshot.count(Phase::Frame), 1);
    assert_eq!(
        snapshot.count(Phase::Plan),
        0,
        "another thread's span stays out of the capture"
    );
    assert_eq!(snapshot.damage_frames(), 1);
    assert_eq!(snapshot.damaged_fraction(), Some(0.25));
}

#[test]
fn a_panic_inside_a_capture_ends_it() {
    let panicked = std::panic::catch_unwind(|| {
        capture(|| {
            drop(span(Phase::Gpu));
            panic!("inside a capture");
        })
    });
    assert!(panicked.is_err());
    assert!(!CAPTURING.with(Cell::get), "this thread no longer captures");

    let ((), next) = capture(|| {});
    assert_eq!(
        next.count(Phase::Gpu),
        0,
        "the abandoned capture's records are gone"
    );
}

#[test]
fn each_capture_starts_empty() {
    let ((), first) = capture(|| drop(span(Phase::Present)));
    let ((), second) = capture(|| {});

    assert_eq!(first.count(Phase::Present), 1);
    assert_eq!(second.count(Phase::Present), 0);
    assert_eq!(second.damaged_fraction(), None);
}
