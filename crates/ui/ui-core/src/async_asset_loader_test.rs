use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use web_time::Instant;

use super::*;
use reactive_core::drain_tasks;

/// Never actually resolves: `load` records the call and parks the `reply` until the test releases it, so a test can tell "the transport was asked" apart from "the transport already answered".
#[derive(Default)]
struct GatedTransport {
    calls: Mutex<Vec<AssetKey>>,
    pending: Mutex<Vec<(AssetKey, Reply)>>,
}

impl AssetTransport for GatedTransport {
    fn load(&self, key: &AssetKey, reply: Reply) {
        self.calls.lock().unwrap().push(key.clone());
        self.pending.lock().unwrap().push((key.clone(), reply));
    }
}

impl GatedTransport {
    fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }

    fn resolve_all_ok(&self, bytes: &[u8]) {
        let pending = std::mem::take(&mut *self.pending.lock().unwrap());
        for (_, reply) in pending {
            reply(Ok(bytes.to_vec()));
        }
    }
}

struct PanicTransport;
impl AssetTransport for PanicTransport {
    fn load(&self, _key: &AssetKey, _reply: Reply) {
        panic!("the cache should have answered before the transport was ever asked");
    }
}

#[derive(Default)]
struct SpyCache {
    stored: Mutex<HashMap<AssetKey, Vec<u8>>>,
    puts: Mutex<Vec<AssetKey>>,
}

impl AssetCache for SpyCache {
    fn get(&self, key: &AssetKey) -> Option<Vec<u8>> {
        self.stored.lock().unwrap().get(key).cloned()
    }
    fn put(&self, key: &AssetKey, bytes: &[u8]) {
        self.puts.lock().unwrap().push(key.clone());
        self.stored
            .lock()
            .unwrap()
            .insert(key.clone(), bytes.to_vec());
    }
}

struct EchoDecoder;
impl AssetDecoder for EchoDecoder {
    fn kind(&self) -> &'static str {
        "echo"
    }
    type Output = Arc<str>;
    fn decode(&self, bytes: &[u8]) -> Result<Self::Output, AssetError> {
        std::str::from_utf8(bytes)
            .map(Arc::from)
            .map_err(|e| AssetError::permanent(e.to_string()))
    }
}

fn wait_until(mut done: impl FnMut() -> bool, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while !done() {
        assert!(Instant::now() < deadline, "condition never became true");
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Drains delivered task values, standing in for the runner's per-frame `drain_tasks`, until `read` leaves `Loading` or the timeout gives up.
fn settle<V: Clone + Send + 'static>(read: &ReadSignal<AssetState<V>>, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    loop {
        drain_tasks();
        if !matches!(read.get(), AssetState::Loading) || Instant::now() > deadline {
            return;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn a_first_read_returns_loading_without_blocking() {
    let loader: AssetLoader<Arc<str>> = AssetLoader::new(
        Arc::new(GatedTransport::default()),
        None,
        Arc::new(EchoDecoder),
    );

    assert!(
        matches!(loader.get("greeting").get(), AssetState::Loading),
        "a first read answers Loading rather than blocking"
    );
}

#[test]
fn repeated_reads_of_the_same_id_share_one_transport_call() {
    let transport = Arc::new(GatedTransport::default());
    let loader: AssetLoader<Arc<str>> = AssetLoader::new(
        Arc::clone(&transport) as Arc<dyn AssetTransport>,
        None,
        Arc::new(EchoDecoder),
    );

    let reads: Vec<_> = (0..5).map(|_| loader.get("hello")).collect();

    wait_until(|| transport.call_count() == 1, Duration::from_secs(5));
    transport.resolve_all_ok(b"hi");
    settle(&reads[0], Duration::from_secs(5));

    assert_eq!(transport.call_count(), 1);
    for (i, read) in reads.iter().enumerate() {
        assert!(
            matches!(read.get(), AssetState::Ready(_)),
            "one request served every reader of the key, so reader {i} must see it too"
        );
    }
}

#[test]
fn a_failed_decode_never_reaches_cache_put() {
    let transport = Arc::new(GatedTransport::default());
    let cache = Arc::new(SpyCache::default());
    let loader: AssetLoader<Arc<str>> = AssetLoader::new(
        Arc::clone(&transport) as Arc<dyn AssetTransport>,
        Some(Arc::clone(&cache) as Arc<dyn AssetCache>),
        Arc::new(EchoDecoder),
    );

    let read = loader.get("bad");
    wait_until(|| transport.call_count() == 1, Duration::from_secs(5));
    transport.resolve_all_ok(&[0xff, 0xfe]);
    settle(&read, Duration::from_secs(5));

    assert!(
        matches!(read.get(), AssetState::Failed(_)),
        "a body that does not decode leaves the read Failed"
    );
    assert!(
        cache.puts.lock().unwrap().is_empty(),
        "a decode failure must never reach AssetCache::put"
    );
}

#[test]
fn a_cache_hit_never_reaches_the_transport() {
    let cache = Arc::new(SpyCache::default());
    let key = AssetKey::new(EchoDecoder.kind(), "cached");
    cache
        .stored
        .lock()
        .unwrap()
        .insert(key, b"already here".to_vec());

    let loader: AssetLoader<Arc<str>> = AssetLoader::new(
        Arc::new(PanicTransport),
        Some(Arc::clone(&cache) as Arc<dyn AssetCache>),
        Arc::new(EchoDecoder),
    );

    let read = loader.get("cached");
    settle(&read, Duration::from_secs(5));

    assert!(
        matches!(read.get(), AssetState::Ready(_)),
        "a cache hit resolves without the transport"
    );
}

/// The in-flight limit governs the queue, not merely deduplication: two *distinct* ids must not both reach the transport at once when the default cap is 1.
#[test]
fn a_second_distinct_id_waits_for_the_first_to_finish() {
    let transport = Arc::new(GatedTransport::default());
    let loader: AssetLoader<Arc<str>> = AssetLoader::new(
        Arc::clone(&transport) as Arc<dyn AssetTransport>,
        None,
        Arc::new(EchoDecoder),
    );

    let a = loader.get("a");
    let _b = loader.get("b");

    wait_until(|| transport.call_count() >= 1, Duration::from_secs(5));
    std::thread::sleep(Duration::from_millis(20));
    assert_eq!(
        transport.call_count(),
        1,
        "a second id must wait its turn rather than starting alongside the first"
    );

    transport.resolve_all_ok(b"x");
    settle(&a, Duration::from_secs(5));
    wait_until(|| transport.call_count() == 2, Duration::from_secs(5));
}

struct DroppingTransport;

impl AssetTransport for DroppingTransport {
    fn load(&self, _key: &AssetKey, reply: Reply) {
        drop(reply);
    }
}

#[derive(Default)]
struct FlakyTransport {
    calls: Mutex<usize>,
    succeed_from: usize,
}

impl AssetTransport for FlakyTransport {
    fn load(&self, _key: &AssetKey, reply: Reply) {
        let mut calls = self.calls.lock().unwrap();
        *calls += 1;
        match *calls >= self.succeed_from {
            true => reply(Ok(b"fine".to_vec())),
            false => reply(Err(AssetError::transient("offline"))),
        }
    }
}

#[test]
fn the_transport_error_reaches_the_signal() {
    let transport = Arc::new(FlakyTransport {
        succeed_from: usize::MAX,
        ..Default::default()
    });
    let loader: AssetLoader<Arc<str>> = AssetLoader::new(transport, None, Arc::new(EchoDecoder));

    let read = loader.get("x");
    settle(&read, Duration::from_secs(5));

    match read.get() {
        AssetState::Failed(error) => assert_eq!(error, AssetError::transient("offline")),
        _ => panic!("expected Failed"),
    }
}

#[test]
fn a_dropped_reply_becomes_a_transient_failure() {
    let loader: AssetLoader<Arc<str>> =
        AssetLoader::new(Arc::new(DroppingTransport), None, Arc::new(EchoDecoder));

    let read = loader.get("x");
    settle(&read, Duration::from_secs(5));

    assert!(matches!(read.get(), AssetState::Failed(error) if error.is_transient()));
}

#[test]
fn a_transient_failure_is_requested_again_only_after_the_backoff() {
    let transport = Arc::new(FlakyTransport {
        succeed_from: 2,
        ..Default::default()
    });
    let loader: AssetLoader<Arc<str>> = AssetLoader::new(
        Arc::clone(&transport) as Arc<dyn AssetTransport>,
        None,
        Arc::new(EchoDecoder),
    )
    .with_retry_backoff(Duration::from_millis(300), Duration::from_secs(5));

    let read = loader.get("x");
    settle(&read, Duration::from_secs(5));
    assert!(matches!(read.get(), AssetState::Failed(_)));

    loader.get("x");
    std::thread::sleep(Duration::from_millis(20));
    drain_tasks();
    assert_eq!(*transport.calls.lock().unwrap(), 1, "too early to retry");
    assert!(matches!(read.get(), AssetState::Failed(_)));

    std::thread::sleep(Duration::from_millis(350));
    let again = loader.get("x");
    assert!(matches!(again.get(), AssetState::Loading));
    settle(&read, Duration::from_secs(5));

    assert_eq!(*transport.calls.lock().unwrap(), 2);
    assert!(matches!(read.get(), AssetState::Ready(_)));
}

#[test]
fn the_backoff_doubles_per_consecutive_failure_up_to_the_cap() {
    let backoff = Backoff {
        base: Duration::from_secs(1),
        max: Duration::from_secs(5),
    };
    let delays: Vec<_> = (1..=5).map(|n| backoff.delay(n).as_secs()).collect();
    assert_eq!(delays, [1, 2, 4, 5, 5]);
}

struct MissingTransport {
    calls: Mutex<usize>,
}

impl AssetTransport for MissingTransport {
    fn load(&self, _key: &AssetKey, reply: Reply) {
        *self.calls.lock().unwrap() += 1;
        reply(Err(AssetError::permanent("404")));
    }
}

#[test]
fn a_permanent_failure_is_never_requested_again() {
    let transport = Arc::new(MissingTransport {
        calls: Mutex::new(0),
    });
    let loader: AssetLoader<Arc<str>> = AssetLoader::new(
        Arc::clone(&transport) as Arc<dyn AssetTransport>,
        None,
        Arc::new(EchoDecoder),
    )
    .with_retry_backoff(Duration::from_millis(50), Duration::from_millis(50));

    let read = loader.get("x");
    settle(&read, Duration::from_secs(5));
    std::thread::sleep(Duration::from_millis(120));

    let again = loader.get("x");
    drain_tasks();

    assert!(
        matches!(again.get(), AssetState::Failed(_)),
        "stays Failed, no flip to Loading"
    );
    assert_eq!(*transport.calls.lock().unwrap(), 1);
}

#[test]
fn a_decode_failure_is_permanent() {
    struct Garbage;
    impl AssetTransport for Garbage {
        fn load(&self, _key: &AssetKey, reply: Reply) {
            reply(Ok(vec![0xff, 0xfe]));
        }
    }
    let loader: AssetLoader<Arc<str>> =
        AssetLoader::new(Arc::new(Garbage), None, Arc::new(EchoDecoder));

    let read = loader.get("x");
    settle(&read, Duration::from_secs(5));

    match read.get() {
        AssetState::Failed(error) => assert!(!error.is_transient()),
        _ => panic!("expected Failed"),
    }
}
