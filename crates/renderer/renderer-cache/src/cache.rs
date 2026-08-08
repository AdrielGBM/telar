use std::cell::Cell;
use std::hash::{Hash, Hasher};
use std::num::NonZeroUsize;
use std::time::{Duration, Instant};

use clru::{CLruCache, CLruCacheConfig, WeightScale};
use rustc_hash::{FxBuildHasher, FxHasher};

use crate::Policy;

/// How many offered-once keys the admission table remembers while waiting to see whether anything asks again. Keys
/// are stored as one `u64` each, so the whole table is a few tens of KB; past it the oldest are forgotten and their
/// values simply pay admission again.
const ADMISSION_TABLE_ENTRIES: usize = 4096;

/// Floor on the gap between idle sweeps, so a cache with a short horizon does not walk itself on every access.
const MIN_SWEEP_INTERVAL: Duration = Duration::from_millis(100);

/// What one cache is holding, for a census something outside the renderer can read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CacheStat {
    pub name: &'static str,
    /// Resident weight, as the cache's weigh function counts it — bytes for every cache Telar builds.
    pub bytes: usize,
    pub entries: usize,
    /// The ceiling `bytes` is measured against, so a reader can tell "full" from "growing".
    pub capacity: usize,
}

/// A value and the moment it was last handed out.
///
/// LRU order alone cannot answer "has anything wanted this lately": it ranks entries against each other, so a cache
/// that never fills keeps its coldest entry forever. In a shell that runs for days that is the difference between a
/// bounded cache and one holding a folder name read twice last Tuesday.
///
/// `Cell` because a weight-budgeted cache cannot hand out `&mut V` — mutating a value would change its weight behind
/// the budget's back — so the timestamp has to be writable through a shared reference.
struct Tracked<V> {
    value: V,
    last_used: Cell<Instant>,
}

struct Scale<V> {
    weigh: fn(&V) -> usize,
}

impl<K, V> WeightScale<K, Tracked<V>> for Scale<V> {
    fn weight(&self, _key: &K, value: &Tracked<V>) -> usize {
        (self.weigh)(&value.value).max(1)
    }
}

fn key_hash<K: Hash>(key: &K) -> u64 {
    let mut hasher = FxHasher::default();
    key.hash(&mut hasher);
    hasher.finish()
}

/// The one cache every Telar renderer backend draws from: a weight-budgeted LRU that also evicts by idle age and can
/// require a second sighting before it keeps anything.
///
/// One type rather than one per backend because the backends had drifted into opposite policies for the same
/// content — the GPU side evicting by frame age with no size cap at all, the CPU side by byte budget with no notion
/// of age — and because four of the GPU caches were hand-rolled reimplementations of the same map-plus-eviction-queue,
/// each with its own handling of entries re-touched since they were queued. What varies between caches is the
/// [`Policy`] and how a value is weighed, which is why both are arguments rather than types.
///
/// The budget is honoured to within one byte per resident entry: the underlying `clru` counts entry *count* alongside
/// weight when deciding whether a value fits. On the smallest budget here that is a fraction of a percent, and it
/// errs toward holding less than asked rather than more.
pub struct Cache<K, V> {
    entries: CLruCache<K, Tracked<V>, FxBuildHasher, Scale<V>>,
    admission: Option<CLruCache<u64, (), FxBuildHasher>>,
    idle: Option<Duration>,
    sweep_interval: Duration,
    last_sweep: Instant,
}

impl<K: Eq + Hash, V> Cache<K, V> {
    /// Builds a cache bounded by `policy`, sizing entries with `weigh`.
    ///
    /// `weigh` is a function rather than a trait implementation so a cache can weigh a foreign type — a `Pixmap`, a
    /// GPU texture — without a newtype standing in the way, and so the four weight-scale structs this replaced
    /// collapse into four one-line closures at their construction sites.
    pub fn new(policy: Policy, weigh: fn(&V) -> usize) -> Self {
        let capacity = NonZeroUsize::new(policy.capacity.max(1)).unwrap();
        let entries = CLruCache::with_config(
            CLruCacheConfig::new(capacity)
                .with_hasher(FxBuildHasher)
                .with_scale(Scale { weigh }),
        );
        let admission = policy.admit_on_second_use.then(|| {
            CLruCache::with_hasher(
                NonZeroUsize::new(ADMISSION_TABLE_ENTRIES).unwrap(),
                FxBuildHasher,
            )
        });
        Self {
            entries,
            admission,
            idle: policy.idle,
            sweep_interval: policy.idle.map_or(MIN_SWEEP_INTERVAL, |idle| {
                (idle / 4).max(MIN_SWEEP_INTERVAL)
            }),
            last_sweep: Instant::now(),
        }
    }

    pub fn get(&mut self, key: &K) -> Option<&V> {
        self.sweep_if_due();
        let entry = self.entries.get(key)?;
        entry.last_used.set(Instant::now());
        Some(&entry.value)
    }

    /// Offers `value` to the cache, returning whether it was kept.
    ///
    /// `false` means either that admission held it back — the first sighting of a key under
    /// [`Policy::admit_on_second_use`] — or that the value alone weighs more than the whole budget. Callers get the
    /// value they were going to draw either way; only whether a later frame finds it again is at stake.
    pub fn insert(&mut self, key: K, value: V) -> bool {
        self.sweep_if_due();
        if let Some(admission) = &mut self.admission {
            let hash = key_hash(&key);
            if admission.pop(&hash).is_none() {
                admission.put(hash, ());
                return false;
            }
        }
        self.entries
            .put_with_weight(
                key,
                Tracked {
                    value,
                    last_used: Cell::new(Instant::now()),
                },
            )
            .is_ok()
    }

    /// Whether `key` is resident, without promoting it. A [`get`](Self::get) that discards its result would count as
    /// a use and reorder eviction; this does not.
    pub fn contains(&self, key: &K) -> bool {
        self.entries.peek(key).is_some()
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        self.entries.pop(key).map(|entry| entry.value)
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        if let Some(admission) = &mut self.admission {
            admission.clear();
        }
    }

    /// Drops everything nothing has asked for within the policy's idle horizon, regardless of when the last sweep
    /// ran. Accessing the cache sweeps on its own schedule; call this to reclaim at a moment the cache cannot see,
    /// such as a surface being hidden or a process going idle with no frames left to draw.
    pub fn sweep(&mut self) {
        let Some(idle) = self.idle else {
            return;
        };
        self.last_sweep = Instant::now();
        let now = Instant::now();
        self.entries
            .retain(|_, entry| now.saturating_duration_since(entry.last_used.get()) < idle);
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// What the resident entries weigh, as the cache's weigh function counts them.
    pub fn weight(&self) -> usize {
        self.entries.weight()
    }

    pub fn capacity(&self) -> usize {
        self.entries.capacity()
    }

    /// What this cache is holding, labelled `name`.
    pub fn stat(&self, name: &'static str) -> CacheStat {
        CacheStat {
            name,
            bytes: self.weight(),
            entries: self.len(),
            capacity: self.capacity(),
        }
    }

    /// Raises the budget to `capacity`, leaving it alone if it is already at least that large.
    ///
    /// Grows only, because a surface-derived budget has to serve every surface sharing the cache: one that shrank to
    /// whichever surface drew last would evict, on a bar's frame, what a full-screen window still needs.
    pub fn grow_to(&mut self, capacity: usize) {
        if capacity > self.entries.capacity() {
            self.entries
                .resize(NonZeroUsize::new(capacity.max(1)).unwrap());
        }
    }

    fn sweep_if_due(&mut self) {
        if self.idle.is_some() && self.last_sweep.elapsed() >= self.sweep_interval {
            self.sweep();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admission_keeps_nothing_until_a_key_is_offered_twice() {
        let mut cache: Cache<&str, Vec<u8>> =
            Cache::new(Policy::new(1024).admit_on_second_use(), Vec::len);

        assert!(!cache.insert("14:32:07", vec![0; 32]));
        assert!(cache.get(&"14:32:07").is_none());

        assert!(cache.insert("14:32:07", vec![0; 32]));
        assert!(cache.get(&"14:32:07").is_some());
    }

    #[test]
    fn without_admission_the_first_offer_is_kept() {
        let mut cache: Cache<&str, Vec<u8>> = Cache::new(Policy::new(1024), Vec::len);

        assert!(cache.insert("icon", vec![0; 32]));
        assert!(cache.get(&"icon").is_some());
    }

    #[test]
    fn the_budget_evicts_least_recently_used_first() {
        let mut cache: Cache<u32, Vec<u8>> = Cache::new(Policy::new(100), Vec::len);
        cache.insert(1, vec![0; 40]);
        cache.insert(2, vec![0; 40]);

        // Promotes 1 over 2, so the next insert has to drop 2 to fit.
        assert!(cache.get(&1).is_some());
        cache.insert(3, vec![0; 40]);

        assert!(cache.get(&1).is_some());
        assert!(cache.get(&2).is_none());
        assert!(cache.get(&3).is_some());
    }

    #[test]
    fn a_value_heavier_than_the_whole_budget_is_refused() {
        let mut cache: Cache<u32, Vec<u8>> = Cache::new(Policy::new(64), Vec::len);

        assert!(!cache.insert(1, vec![0; 128]));
        assert!(cache.is_empty());
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

    #[test]
    fn the_idle_sweep_drops_what_nothing_asked_for() {
        let idle = Duration::from_millis(120);
        let mut cache: Cache<u32, Vec<u8>> = Cache::new(Policy::new(4096).idle(idle), Vec::len);
        cache.insert(1, vec![0; 32]);
        cache.insert(2, vec![0; 32]);

        std::thread::sleep(idle / 2);
        assert!(cache.get(&1).is_some());
        std::thread::sleep(idle);

        cache.sweep();
        assert!(
            cache.get(&1).is_none(),
            "even a re-touched entry goes once it too falls past the horizon"
        );
        assert!(cache.get(&2).is_none());
    }

    #[test]
    fn a_budget_with_no_idle_horizon_holds_a_cold_entry_indefinitely() {
        let mut cache: Cache<u32, Vec<u8>> = Cache::new(Policy::new(4096), Vec::len);
        cache.insert(1, vec![0; 32]);

        std::thread::sleep(Duration::from_millis(50));
        cache.sweep();

        assert!(cache.get(&1).is_some());
    }

    #[test]
    fn contains_does_not_count_as_a_use() {
        let mut cache: Cache<u32, Vec<u8>> = Cache::new(Policy::new(100), Vec::len);
        cache.insert(1, vec![0; 40]);
        cache.insert(2, vec![0; 40]);

        assert!(cache.contains(&1));
        cache.insert(3, vec![0; 40]);

        assert!(
            !cache.contains(&1),
            "peeking left 1 as the least recently used"
        );
        assert!(cache.contains(&2));
    }

    #[test]
    fn a_key_evicted_after_admission_has_to_earn_it_again() {
        let mut cache: Cache<u32, Vec<u8>> =
            Cache::new(Policy::new(100).admit_on_second_use(), Vec::len);
        assert!(!cache.insert(1, vec![0; 40]));
        assert!(cache.insert(1, vec![0; 40]));

        cache.insert(2, vec![0; 40]);
        cache.insert(2, vec![0; 40]);
        cache.insert(3, vec![0; 40]);
        cache.insert(3, vec![0; 40]);
        assert!(cache.get(&1).is_none());

        assert!(!cache.insert(1, vec![0; 40]));
    }
}
