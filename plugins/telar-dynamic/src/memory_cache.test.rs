use super::*;

fn key(kind: &'static str, id: &str) -> AssetKey {
    AssetKey::new(kind, id)
}

#[test]
fn what_went_in_comes_back() {
    let cache = MemoryCache::new(1024);
    cache.put(&key("svg", "logo"), b"<svg/>");
    assert_eq!(
        cache.get(&key("svg", "logo")).as_deref(),
        Some(&b"<svg/>"[..])
    );
}

#[test]
fn an_id_nobody_cached_is_a_miss() {
    let cache = MemoryCache::new(1024);
    assert!(cache.get(&key("svg", "absent")).is_none());
}

/// The collision the `kind` segment exists to prevent, checked here too: the cache is keyed on the whole [`AssetKey`], so an SVG and a bitmap sharing an id are two entries.
#[test]
fn two_formats_with_the_same_id_do_not_share_an_entry() {
    let cache = MemoryCache::new(1024);
    cache.put(&key("svg", "logo"), b"vector");
    cache.put(&key("image", "logo"), b"raster");
    assert_eq!(
        cache.get(&key("svg", "logo")).as_deref(),
        Some(&b"vector"[..])
    );
    assert_eq!(
        cache.get(&key("image", "logo")).as_deref(),
        Some(&b"raster"[..])
    );
}

/// The bound is bytes, not entries: what a cache of this shape has to promise is a ceiling on memory, and an entry count cannot give one when entries are whole files.
#[test]
fn the_budget_is_enforced_in_bytes() {
    let cache = MemoryCache::new(4096);
    for i in 0..64 {
        cache.put(&key("image", &format!("asset-{i}")), &vec![0u8; 512]);
    }
    let stat = cache.stat();
    assert!(
        stat.bytes <= 4096,
        "held {} bytes against a 4096-byte budget",
        stat.bytes
    );
    assert!(
        stat.entries < 64,
        "64 × 512 bytes cannot fit in 4096 and nothing was evicted"
    );
}

/// Eviction is least-recently-used, so re-reading an entry is what keeps it: a cache that evicted by insertion order would drop the icon on screen in favour of the one scrolled past.
#[test]
fn a_re_read_entry_outlives_one_nobody_asked_for() {
    let cache = MemoryCache::new(2048);
    cache.put(&key("image", "kept"), &vec![1u8; 512]);
    cache.put(&key("image", "cold"), &vec![2u8; 512]);
    assert!(cache.get(&key("image", "kept")).is_some());
    for i in 0..8 {
        cache.put(&key("image", &format!("filler-{i}")), &vec![3u8; 512]);
    }
    assert!(
        cache.get(&key("image", "cold")).is_none(),
        "the entry nothing asked for should have gone first"
    );
}

/// Bytes bigger than the whole budget are declined rather than emptying the cache to make room for something that still will not fit.
#[test]
fn a_value_larger_than_the_budget_does_not_evict_everything() {
    let cache = MemoryCache::new(1024);
    cache.put(&key("image", "small"), &vec![0u8; 256]);
    cache.put(&key("image", "huge"), &vec![0u8; 8192]);
    assert!(cache.get(&key("image", "huge")).is_none());
    assert!(
        cache.get(&key("image", "small")).is_some(),
        "an oversized value must not take the cache down with it"
    );
}

#[test]
fn clear_drops_everything() {
    let cache = MemoryCache::new(1024);
    cache.put(&key("svg", "logo"), b"<svg/>");
    cache.clear();
    assert!(cache.get(&key("svg", "logo")).is_none());
    assert_eq!(cache.stat().entries, 0);
}

/// The seam requires `Send + Sync`, and a cache that cannot be installed is not a cache. This fails to compile rather than at run time if the interior mutability ever regresses to `RefCell`.
#[test]
fn it_satisfies_the_seam() {
    fn assert_installable<C: AssetCache>(_: C) {}
    assert_installable(MemoryCache::default());
}
