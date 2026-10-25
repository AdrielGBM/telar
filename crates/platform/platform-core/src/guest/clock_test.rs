use super::*;

#[test]
fn an_instant_reads_back_from_its_timestamp_on_either_side_of_the_epoch() {
    let epoch = Epoch::start();
    // Not -1: Windows reads two instants within one performance-counter tick as the same instant.
    for nanos in [0, 1, 16_666_667, -1_000_000, -5_000_000_000] {
        let instant = epoch.instant(Timestamp { nanos });
        assert_eq!(epoch.stamp(instant), Timestamp { nanos });
        assert_eq!(epoch.instant(epoch.stamp(instant)), instant);
    }
}

#[test]
fn time_between_timestamps_is_the_time_between_their_instants() {
    let epoch = Epoch::start();
    let earlier = Instant::now();
    let later = earlier + Duration::from_millis(250);
    assert_eq!(
        epoch.stamp(later).nanos - epoch.stamp(earlier).nanos,
        250_000_000
    );
}

#[test]
fn an_adopted_epoch_reads_an_instant_where_the_host_wrote_it() {
    let host = Epoch::start();
    std::thread::sleep(Duration::from_millis(2));
    let guest = Epoch::adopt(host.now());
    let instant = Instant::now() + Duration::from_secs(3);
    let read = guest.instant(host.stamp(instant));
    let off = read
        .checked_duration_since(instant)
        .unwrap_or_else(|| instant.duration_since(read));
    assert!(off < Duration::from_millis(5), "{off:?} apart");
}

#[test]
fn a_timestamp_beyond_what_fits_saturates() {
    let epoch = Epoch::start();
    let far = epoch.instant(Timestamp { nanos: i64::MAX });
    assert_eq!(epoch.stamp(far).nanos, i64::MAX);
}
