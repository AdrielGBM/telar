use super::*;
use motion_core::Tickable;
use std::time::Duration;

fn run(glide: &Glide, frames: u32) {
    let mut now = Instant::now();
    glide.tick(now, 1.0);
    for _ in 0..frames {
        now += Duration::from_millis(16);
        glide.tick(now, 1.0);
    }
}

/// The gap is widest at the start, so the first frame already covers a good part of the notch. A glide that eased in from nothing would read as the wheel not having taken.
#[test]
fn a_notch_starts_moving_on_the_frame_after_it_is_turned() {
    let offset = reactive_core::signal(0.0);
    let glide = Glide::start(offset, 60.0, (0.0, 1000.0)).expect("a notch to cover");
    run(&glide, 1);
    let after_one = offset.peek();
    assert!(
        after_one > 6.0,
        "a tenth of the way at least, not a standing start: {after_one}"
    );
    assert!(after_one < 60.0, "and not the whole notch at once");
}

#[test]
fn it_arrives_exactly_where_the_notch_asked() {
    let offset = reactive_core::signal(0.0);
    let glide = Glide::start(offset, 60.0, (0.0, 1000.0)).expect("a notch to cover");
    run(&glide, 30);
    assert_eq!(offset.peek(), 60.0);
    assert!(
        glide.is_settled(),
        "the glide ends on the notch it was given"
    );
}

/// Turning again mid-glide means further, not instead: restarting from wherever the content had reached would drop the ground the first notch had not covered, so spinning the wheel fast would travel less than turning it slowly.
#[test]
fn a_second_notch_adds_to_the_first_rather_than_replacing_it() {
    let offset = reactive_core::signal(0.0);
    let glide = Glide::start(offset, 60.0, (0.0, 1000.0)).expect("a notch to cover");
    run(&glide, 1);
    assert!(
        glide.extend(60.0, (0.0, 1000.0)),
        "a second notch is accepted while the first is still running"
    );
    run(&glide, 30);
    assert_eq!(offset.peek(), 120.0, "both notches, not the later one");
}

#[test]
fn it_stops_at_the_end_of_the_content() {
    let offset = reactive_core::signal(0.0);
    let glide = Glide::start(offset, 500.0, (0.0, 80.0)).expect("a notch to cover");
    run(&glide, 30);
    assert_eq!(offset.peek(), 80.0);
}

#[test]
fn a_notch_at_the_edge_starts_nothing() {
    let offset = reactive_core::signal(80.0);
    assert!(
        Glide::start(offset, 500.0, (0.0, 80.0)).is_none(),
        "a notch into the edge has nowhere to go"
    );
}

/// Whatever took the offset over says where it goes; a glide finishing afterwards would pull it back.
#[test]
fn a_stopped_glide_takes_no_more_notches_and_moves_no_further() {
    let offset = reactive_core::signal(0.0);
    let glide = Glide::start(offset, 60.0, (0.0, 1000.0)).expect("a notch to cover");
    run(&glide, 1);
    let caught = offset.peek();
    glide.stop();
    assert!(
        !glide.extend(60.0, (0.0, 1000.0)),
        "a stopped glide takes no further notches"
    );
    run(&glide, 20);
    assert_eq!(offset.peek(), caught);
}
