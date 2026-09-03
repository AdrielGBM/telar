//! Where each rendering thread leaves a census of its own caches, so something else can read it.
//!
//! A relay, not a second set of books. The caches are thread-local — a software renderer is thread-bound because
//! it owns a softbuffer `Surface`, and a hardware one because it owns a wgpu device — so a reader on any other
//! thread cannot see them at all. Asking for them from an IPC handler would build a fresh empty set on that
//! thread and report zeros, which is worse than reporting nothing.
//!
//! It lives here rather than in either backend because both have caches worth counting, and a census that covered
//! only one would be wrong by omission in exactly the case where it mattered: a shell resolving to hardware would
//! read "nothing cached" while its GPU caches filled.

use std::cell::Cell;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::thread::ThreadId;
use std::time::Duration;
use web_time::Instant;

use crate::CacheStat;

/// How stale a published census may be. A memory census does not need frame resolution, and this keeps a lock off
/// the frame path 59 times out of 60.
const PUBLISH_EVERY: Duration = Duration::from_secs(1);

/// Who a census came from, so two reports of the same caches are not counted twice.
///
/// A thread-local set is one per thread and its reports add up. A process-wide set is one, however many threads
/// report it: the GPU backend renders each surface on its own thread but shares one cache set between them, so
/// keying those by thread multiplied every figure by the number of render threads.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Origin {
    Thread(ThreadId),
    Shared(&'static str),
}

fn published() -> &'static Mutex<HashMap<Origin, Vec<CacheStat>>> {
    static PUBLISHED: OnceLock<Mutex<HashMap<Origin, Vec<CacheStat>>>> = OnceLock::new();
    PUBLISHED.get_or_init(|| Mutex::new(HashMap::new()))
}

thread_local! {
    static LAST_PUBLISH: Cell<Option<Instant>> = const { Cell::new(None) };
}

/// Whether enough time has passed for this thread to publish again. Call before gathering the stats, so a thread
/// that is not due skips the gathering too.
pub fn publish_due() -> bool {
    LAST_PUBLISH.with(|last| match last.get() {
        Some(at) if at.elapsed() < PUBLISH_EVERY => false,
        _ => {
            last.set(Some(Instant::now()));
            true
        }
    })
}

/// Leaves this thread's cache census where another thread can read it, for caches held per thread.
pub fn publish(stats: Vec<CacheStat>) {
    if let Ok(mut published) = published().lock() {
        published.insert(Origin::Thread(std::thread::current().id()), stats);
    }
}

/// Publishes a census of caches shared process-wide under `name`, so every thread reporting them lands on the same
/// entry and the figures are counted once rather than once per reporter.
pub fn publish_shared(name: &'static str, stats: Vec<CacheStat>) {
    if let Ok(mut published) = published().lock() {
        published.insert(Origin::Shared(name), stats);
    }
}

/// Every rendering thread's caches, summed by name.
///
/// Summed because the caches are per thread: a shell rendering surfaces on two threads holds two raster caches,
/// and the number worth reading is what the process holds. Never more than [`PUBLISH_EVERY`] out of date, and
/// empty until some thread has drawn a frame.
pub fn snapshot() -> Vec<CacheStat> {
    let Ok(published) = published().lock() else {
        return Vec::new();
    };
    let mut totals: Vec<CacheStat> = Vec::new();
    for stats in published.values() {
        for stat in stats {
            match totals.iter_mut().find(|total| total.name == stat.name) {
                Some(total) => {
                    total.bytes += stat.bytes;
                    total.entries += stat.entries;
                    total.capacity += stat.capacity;
                }
                None => totals.push(*stat),
            }
        }
    }
    // Sorted so the census reads the same twice running: the map is iterated in hash order.
    totals.sort_by_key(|stat| stat.name);
    totals
}
