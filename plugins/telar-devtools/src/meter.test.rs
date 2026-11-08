use super::*;

fn at(start: Instant, millis: u64) -> Instant {
    start + Duration::from_millis(millis)
}

#[test]
fn only_frames_with_content_are_counted() {
    let start = Instant::now();
    let mut meter = FrameMeter::default();
    meter.sample(at(start, 0), true);
    meter.sample(at(start, 10), false);
    let reading = meter.sample(at(start, 20), true);
    assert_eq!(reading.fps, 2);
    assert_eq!(reading.frame_millis, 20.0);
}

#[test]
fn frames_older_than_a_second_fall_out_of_the_count() {
    let start = Instant::now();
    let mut meter = FrameMeter::default();
    meter.sample(at(start, 0), true);
    meter.sample(at(start, 500), true);
    assert_eq!(meter.sample(at(start, 1200), false).fps, 1);
    assert_eq!(meter.sample(at(start, 1600), false).fps, 0);
}

#[test]
fn the_reading_goes_stale_once_a_frame_ages_out() {
    let start = Instant::now();
    let mut meter = FrameMeter::default();
    meter.sample(at(start, 0), true);
    assert!(!meter.is_stale(at(start, 900)), "the frame still counts");
    assert!(
        meter.is_stale(at(start, 1100)),
        "a still app's readout has to fall"
    );
    meter.sample(at(start, 1100), false);
    assert!(
        !meter.is_stale(at(start, 5000)),
        "and once it shows zero it rests"
    );
}
