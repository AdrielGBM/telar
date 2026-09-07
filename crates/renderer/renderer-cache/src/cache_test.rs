use super::*;

#[test]
fn admission_keeps_nothing_until_a_key_is_offered_twice() {
    let mut cache: Cache<&str, Vec<u8>> =
        Cache::new(Policy::new(1024).admit_on_second_use(), Vec::len);

    assert!(
        !cache.insert("14:32:07", vec![0; 32]),
        "a first offer is only recorded, never stored"
    );
    assert!(
        cache.get(&"14:32:07").is_none(),
        "so there is nothing to hand back yet"
    );

    assert!(
        cache.insert("14:32:07", vec![0; 32]),
        "the second offer of the same key admits it"
    );
    assert!(
        cache.get(&"14:32:07").is_some(),
        "an admitted key reads back"
    );
}

#[test]
fn without_admission_the_first_offer_is_kept() {
    let mut cache: Cache<&str, Vec<u8>> = Cache::new(Policy::new(1024), Vec::len);

    assert!(
        cache.insert("icon", vec![0; 32]),
        "with no admission policy the first offer is stored outright"
    );
    assert!(cache.get(&"icon").is_some(), "and reads back immediately");
}

#[test]
fn the_budget_evicts_least_recently_used_first() {
    let mut cache: Cache<u32, Vec<u8>> = Cache::new(Policy::new(100), Vec::len);
    cache.insert(1, vec![0; 40]);
    cache.insert(2, vec![0; 40]);

    // This read is also what makes 1 more recent than 2, which is what the eviction below turns on.
    assert!(
        cache.get(&1).is_some(),
        "1 is still held before the squeeze"
    );
    cache.insert(3, vec![0; 40]);

    assert!(cache.get(&1).is_some(), "the read above spared 1");
    assert!(
        cache.get(&2).is_none(),
        "2 was the least recently used when 3 needed the room"
    );
    assert!(
        cache.get(&3).is_some(),
        "the entry that forced the eviction"
    );
}

#[test]
fn a_value_heavier_than_the_whole_budget_is_refused() {
    let mut cache: Cache<u32, Vec<u8>> = Cache::new(Policy::new(64), Vec::len);

    assert!(
        !cache.insert(1, vec![0; 128]),
        "no eviction can make room for a value larger than the budget itself"
    );
    assert!(
        cache.is_empty(),
        "refusing it must leave nothing behind, held {} entries",
        cache.len()
    );
}

#[test]
fn a_fixed_per_entry_cost_bounds_a_cache_of_small_values() {
    let mut cache: Cache<u32, Vec<u8>> = Cache::new(Policy::new(3200), |_| 32);
    for key in 0..200 {
        cache.insert(key, vec![0; 8]);
    }

    assert!(cache.len() <= 100, "held {} entries", cache.len());
    assert!(cache.len() >= 96, "held {} entries", cache.len());
}

/// Only ever sleeps *past* the horizon, never up to some fraction of it.
///
/// `thread::sleep` guarantees a floor, not a duration: a loaded CI runner turned a 60 ms sleep into more than 120 ms and evicted an entry this test had asserted was still there. Oversleeping cannot break "this should be gone" — only "this should still be here", which is why that direction is tested without a clock at all.
#[test]
fn the_idle_sweep_drops_what_nothing_asked_for() {
    let idle = Duration::from_millis(50);
    let mut cache: Cache<u32, Vec<u8>> = Cache::new(Policy::new(4096).idle(idle), Vec::len);
    cache.insert(1, vec![0; 32]);
    cache.insert(2, vec![0; 32]);

    std::thread::sleep(idle * 3);
    cache.sweep();

    assert!(cache.get(&1).is_none(), "1 sat idle past the horizon");
    assert!(cache.get(&2).is_none(), "and so did 2");
}

#[test]
fn a_sweep_keeps_what_was_just_used() {
    let mut cache: Cache<u32, Vec<u8>> =
        Cache::new(Policy::new(4096).idle(Duration::from_secs(60)), Vec::len);
    cache.insert(1, vec![0; 32]);

    cache.sweep();

    assert!(
        cache.get(&1).is_some(),
        "a minute cannot have passed, so the sweep has nothing to drop"
    );
}

#[test]
fn a_budget_with_no_idle_horizon_holds_a_cold_entry_indefinitely() {
    let mut cache: Cache<u32, Vec<u8>> = Cache::new(Policy::new(4096), Vec::len);
    cache.insert(1, vec![0; 32]);

    std::thread::sleep(Duration::from_millis(50));
    cache.sweep();

    assert!(
        cache.get(&1).is_some(),
        "with no horizon set, age alone is not a reason to evict"
    );
}

#[test]
fn contains_does_not_count_as_a_use() {
    let mut cache: Cache<u32, Vec<u8>> = Cache::new(Policy::new(100), Vec::len);
    cache.insert(1, vec![0; 40]);
    cache.insert(2, vec![0; 40]);

    assert!(cache.contains(&1), "1 is there to peek at");
    cache.insert(3, vec![0; 40]);

    assert!(
        !cache.contains(&1),
        "peeking left 1 as the least recently used"
    );
    assert!(cache.contains(&2), "2 was used more recently, so it stays");
}

#[test]
fn a_key_evicted_after_admission_has_to_earn_it_again() {
    let mut cache: Cache<u32, Vec<u8>> =
        Cache::new(Policy::new(100).admit_on_second_use(), Vec::len);
    assert!(!cache.insert(1, vec![0; 40]), "first offer of 1: recorded");
    assert!(cache.insert(1, vec![0; 40]), "second offer admits it");

    cache.insert(2, vec![0; 40]);
    cache.insert(2, vec![0; 40]);
    cache.insert(3, vec![0; 40]);
    cache.insert(3, vec![0; 40]);
    assert!(
        cache.get(&1).is_none(),
        "1 was evicted to make room for 2 and 3"
    );

    assert!(
        !cache.insert(1, vec![0; 40]),
        "eviction took its admission record too, so 1 starts over"
    );
}
