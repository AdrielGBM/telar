//! Integration test for the HTTP path: the loader, [`HttpTransport`], [`DiskCache`] and [`SvgDecoder`], end to end.
//!
//! The unit tests cover the cache-name rule and the URL template in isolation. What this proves is the part that only shows up wired together — that a read returns `Loading` without blocking, that the transport's answer reaches the signal through `drain_tasks` on this thread, and that the second run resolves from disk with no request reaching the server at all. A transport that only ever spun would pass every unit test in the crate.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use reactive_core::drain_tasks;
use renderer_assets::SvgData;
use telar_dynamic::{AssetKey, AssetTransport, DiskCache, HttpTransport, Reply, SvgDecoder};
use ui_core::{AssetCache, AssetLoader, AssetState};

const SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M2 2 H22 V22 H2 Z"/></svg>"#;

/// A one-endpoint HTTP server on a loopback port, counting what it served so a cache hit is distinguishable from a second download rather than merely plausible.
struct Server {
    port: u16,
    hits: Arc<AtomicUsize>,
}

impl Server {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let port = listener.local_addr().unwrap().port();
        let hits = Arc::new(AtomicUsize::new(0));
        let served = Arc::clone(&hits);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf);
                let found = String::from_utf8_lossy(&buf).contains("/known.svg");
                let response = if found {
                    served.fetch_add(1, Ordering::Relaxed);
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: image/svg+xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{SVG}",
                        SVG.len()
                    )
                } else {
                    "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        .to_string()
                };
                let _ = stream.write_all(response.as_bytes());
            }
        });
        Self { port, hits }
    }

    fn template(&self) -> String {
        format!("http://127.0.0.1:{}/{{name}}.svg", self.port)
    }
}

/// Answers every request with the same bytes, whatever they are, so a decode failure can be provoked without a server that serves one.
struct FixedTransport(Vec<u8>);

impl AssetTransport for FixedTransport {
    fn load(&self, _key: &AssetKey, reply: Reply) {
        reply(Ok(self.0.clone()));
    }
}

/// A [`DiskCache`] that also records every `put` it is offered, so "nothing was written" is asserted at the seam rather than inferred from an absent file.
struct WatchedCache {
    inner: DiskCache,
    puts: Mutex<Vec<AssetKey>>,
}

impl AssetCache for WatchedCache {
    fn get(&self, key: &AssetKey) -> Option<Vec<u8>> {
        self.inner.get(key)
    }

    fn put(&self, key: &AssetKey, bytes: &[u8]) {
        self.puts.lock().unwrap().push(key.clone());
        self.inner.put(key, bytes);
    }
}

fn loader(
    transport: Arc<dyn AssetTransport>,
    cache: Option<Arc<dyn AssetCache>>,
) -> AssetLoader<Arc<SvgData>> {
    AssetLoader::new(transport, cache, Arc::new(SvgDecoder))
}

/// Drains delivered task values until `id` settles, or gives up. Stands in for the runner's per-frame `drain_tasks`, which is the only thing this test needs from a running app.
fn settle(
    loader: &AssetLoader<Arc<SvgData>>,
    id: &str,
    timeout: Duration,
) -> AssetState<Arc<SvgData>> {
    let deadline = Instant::now() + timeout;
    loop {
        drain_tasks();
        let state = loader.get(id).get();
        if !matches!(state, AssetState::Loading) || Instant::now() > deadline {
            return state;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// `SvgData` has no `Debug`, and a failure message that says which of the three states was reached is the whole point of asserting on it.
fn name_of(state: &AssetState<Arc<SvgData>>) -> &'static str {
    match state {
        AssetState::Loading => "Loading",
        AssetState::Ready(_) => "Ready",
        AssetState::Failed => "Failed",
    }
}

fn cache_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("telar-http-assets-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn a_fetched_asset_lands_in_its_signal_and_then_in_the_cache() {
    let server = Server::start();
    let dir = cache_dir("fetched");
    let icons = loader(
        Arc::new(HttpTransport::new(server.template())),
        Some(Arc::new(DiskCache::new(&dir))),
    );

    assert!(
        matches!(icons.get("known").get(), AssetState::Loading),
        "the first read must return without waiting for the network"
    );

    let state = settle(&icons, "known", Duration::from_secs(10));
    assert!(
        state.is_ready(),
        "asset did not resolve: {}",
        name_of(&state)
    );
    assert_eq!(server.hits.load(Ordering::Relaxed), 1);
    assert!(
        dir.join("svg").join("known").exists(),
        "download was not cached under <root>/<kind>/<name>"
    );

    // A second loader over the same directory answers from disk: the server count stays where it was.
    let offline = loader(
        Arc::new(HttpTransport::new(server.template())),
        Some(Arc::new(DiskCache::new(&dir))),
    );
    assert!(
        settle(&offline, "known", Duration::from_secs(10)).is_ready(),
        "the fetched asset settles into its signal"
    );
    assert_eq!(
        server.hits.load(Ordering::Relaxed),
        1,
        "the cached asset was downloaded again"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_asset_the_endpoint_does_not_have_settles_on_failed() {
    let server = Server::start();
    let dir = cache_dir("missing");
    // Two attempts, back to back: the retry path runs, and the test does not wait the default 4s for it.
    let icons = loader(
        Arc::new(HttpTransport::new(server.template()).with_retry(2, Duration::from_millis(20))),
        Some(Arc::new(DiskCache::new(&dir))),
    );

    let state = settle(&icons, "absent", Duration::from_secs(10));
    assert!(
        matches!(state, AssetState::Failed),
        "a missing asset must settle rather than spin forever, got {}",
        name_of(&state)
    );
    assert!(
        !dir.join("svg").join("absent").exists(),
        "a failed fetch must not be cached"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// Reading the same id twice must not queue it twice, however many widgets ask.
#[test]
fn repeated_reads_share_one_request() {
    let server = Server::start();
    let dir = cache_dir("shared");
    let icons = loader(
        Arc::new(HttpTransport::new(server.template())),
        Some(Arc::new(DiskCache::new(&dir))),
    );

    for _ in 0..5 {
        let _ = icons.get("known").get();
    }
    assert!(
        settle(&icons, "known", Duration::from_secs(10)).is_ready(),
        "every reader of the key settles on the shared result"
    );
    assert_eq!(server.hits.load(Ordering::Relaxed), 1);

    let _ = std::fs::remove_dir_all(&dir);
}

/// The property the old worker held with an `if` and the seam now holds by construction: bytes reach the cache only after they have decoded. A provider's 404 page is a 200 with HTML in it often enough that caching the response would make one typo permanent.
#[test]
fn a_body_that_does_not_decode_never_reaches_the_cache() {
    let dir = cache_dir("undecodable");
    let cache = Arc::new(WatchedCache {
        inner: DiskCache::new(&dir),
        puts: Mutex::new(Vec::new()),
    });
    let icons = loader(
        Arc::new(FixedTransport(b"<html>404 Not Found</html>".to_vec())),
        Some(Arc::clone(&cache) as Arc<dyn AssetCache>),
    );

    let state = settle(&icons, "known", Duration::from_secs(10));
    assert!(
        matches!(state, AssetState::Failed),
        "a body that is not an SVG must fail, got {}",
        name_of(&state)
    );
    assert!(
        cache.puts.lock().unwrap().is_empty(),
        "a decode failure reached AssetCache::put"
    );
    assert!(
        !dir.join("svg").join("known").exists(),
        "a body that does not decode must not be cached"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// The failure a decode failure must not be confused with: the same wiring, with bytes that do decode, writes exactly one file.
#[test]
fn a_body_that_decodes_does_reach_the_cache() {
    let dir = cache_dir("decodable");
    let icons = loader(
        Arc::new(FixedTransport(SVG.as_bytes().to_vec())),
        Some(Arc::new(DiskCache::new(&dir))),
    );

    assert!(
        settle(&icons, "known", Duration::from_secs(10)).is_ready(),
        "a body that decodes settles ready"
    );
    assert_eq!(
        std::fs::read(dir.join("svg").join("known")).expect("the decoded body was cached"),
        SVG.as_bytes()
    );

    let _ = std::fs::remove_dir_all(&dir);
}
