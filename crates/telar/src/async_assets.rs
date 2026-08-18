//! An HTTP implementation of the [`AssetSource`] seam: SVGs fetched from a URL template, cached on disk, and
//! delivered into signals on the UI thread.
//!
//! The seam in `ui-core` says what a reactive asset *is* — a signal advancing `Loading` → `Ready`/`Failed` —
//! and deliberately ships no transport. This is the transport every consumer of it was writing anyway: an
//! icon CDN addressed by name, a directory of what has already been downloaded, and a retry for the shell
//! that starts before the network is up.
//!
//! Nothing here touches the frame thread. One worker owns the socket, the disk cache and the SVG parse, and
//! reaches the signals through [`reactive_core::spawn_stream`] — the same bridge [`crate::files`] uses, for
//! the same reason: signals are `!Send`, so the *data* crosses the thread boundary and the UI thread writes.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use reactive_core::{Emitter, ReadSignal, RwSignal, Task, signal, spawn_stream};
use renderer_assets::SvgData;
use ui_core::{AssetSource, AssetState};

/// A transient failure — the shell often starts before the network is up at login — keeps the asset on
/// `Loading` and is retried, so it self-heals once connectivity arrives without hammering the endpoint over
/// a genuine 404.
const DEFAULT_ATTEMPTS: u32 = 8;
const DEFAULT_RETRY_DELAY: Duration = Duration::from_secs(4);
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);

/// A cache file name that is a file name: everything outside `[A-Za-z0-9_-]` becomes `_`, and anything that
/// was not already such a name carries a hash of the original so two ids cannot collapse onto one file.
///
/// The hash is not decoration. Ids arrive from application config (`mdi:home`, `lucide/arrow-right`), so
/// without it `a:b` and `a/b` would share a file, and `../../x` would name a path outside the cache at all.
fn cache_file_name(id: &str) -> String {
    let simple = id.len() <= 32
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if simple {
        return format!("{id}.svg");
    }

    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    id.hash(&mut hasher);
    let hash = hasher.finish();

    let sanitized: String = id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .take(32)
        .collect();
    format!("{sanitized}_{hash:x}.svg")
}

/// What the worker owns. `Send` by construction — no signals, no `Rc`.
#[derive(Clone)]
struct Fetch {
    url_template: String,
    cache_dir: Option<PathBuf>,
    max_attempts: u32,
    retry_delay: Duration,
    timeout: Duration,
}

type Delivery = (String, Option<Arc<SvgData>>);

#[derive(Default)]
struct Store {
    states: RefCell<HashMap<String, RwSignal<AssetState<Arc<SvgData>>>>>,
}

/// An [`AssetSource`] that resolves each id against a URL template and keeps what it downloads on disk.
///
/// `url_template` is the endpoint with `{name}` standing in for the id — `"https://api.iconify.design/{name}.svg"`
/// resolves the id `"mdi/home"` to `https://api.iconify.design/mdi/home.svg`. What an id *means* stays with
/// the application: a set-qualified icon name, a default set, a theme prefix are all decisions this cannot
/// make for it, and all of them come out as the id it is handed.
///
/// Construct it on the UI thread and keep it there (a `thread_local!`, or a field of the app): it holds the
/// signals it hands out, so it is `!Send` like everything else on this side of the seam. Dropping it retires
/// the worker and cancels delivery, which is what a surface tearing down mid-download wants.
///
/// ```ignore
/// let icons = HttpAssetSource::new("https://api.iconify.design/{name}.svg")
///     .cached_in(telar::paths::cache().unwrap_or_else(std::env::temp_dir).join("icons"));
/// // In a view: `Loading` until it lands, then the parsed SVG, re-rendering whoever read it.
/// match icons.svg("mdi/home").get() { .. }
/// ```
pub struct HttpAssetSource {
    store: Rc<Store>,
    requests: Sender<String>,
    /// `Some` until the source is dropped; cancelling it releases the delivery callback, and with it the last
    /// reference to [`Store`].
    task: RefCell<Option<Task>>,
    fetch: Fetch,
    started: RefCell<bool>,
    /// The receiving end, held until the worker starts. Kept rather than started in `new` so a source can be
    /// built before the reactive runtime exists — a thread-local initializer, a test — without the worker
    /// registering its callback against the wrong surface.
    incoming: RefCell<Option<Receiver<String>>>,
}

impl HttpAssetSource {
    /// A source that fetches from `url_template` and caches nothing — every miss is a round trip. Pair it
    /// with [`cached_in`](Self::cached_in) unless the assets genuinely should not touch disk.
    pub fn new(url_template: impl Into<String>) -> Self {
        let (requests, incoming) = mpsc::channel();
        Self {
            store: Rc::new(Store::default()),
            requests,
            task: RefCell::new(None),
            fetch: Fetch {
                url_template: url_template.into(),
                cache_dir: None,
                max_attempts: DEFAULT_ATTEMPTS,
                retry_delay: DEFAULT_RETRY_DELAY,
                timeout: DEFAULT_TIMEOUT,
            },
            started: RefCell::new(false),
            incoming: RefCell::new(Some(incoming)),
        }
    }

    /// Keeps every downloaded asset under `dir`, so a restart resolves from disk instead of the network.
    /// The directory is created on first write.
    pub fn cached_in(mut self, dir: impl Into<PathBuf>) -> Self {
        self.fetch.cache_dir = Some(dir.into());
        self
    }

    /// How many times a failing fetch is retried, and how long between attempts. `attempts` counts the first
    /// try, so `1` disables retrying; running out settles the asset on [`AssetState::Failed`].
    pub fn with_retry(mut self, attempts: u32, delay: Duration) -> Self {
        self.fetch.max_attempts = attempts.max(1);
        self.fetch.retry_delay = delay;
        self
    }

    /// How long one request may take before it counts as a failure. Defaults to 15s.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.fetch.timeout = timeout;
        self
    }

    fn ensure_worker(&self) {
        if *self.started.borrow() {
            return;
        }
        let Some(incoming) = self.incoming.borrow_mut().take() else {
            return;
        };
        *self.started.borrow_mut() = true;

        let fetch = self.fetch.clone();
        let store = Rc::clone(&self.store);
        let task = spawn_stream(
            move |out| run_worker(incoming, fetch, out),
            move |(id, data): Delivery| {
                // Clone the handle out and drop the borrow BEFORE `set`: a signal write flushes effects
                // synchronously, which re-renders a widget → `svg()` → `states.borrow_mut()`, and holding the
                // borrow across the write would re-enter and panic.
                let handle = store.states.borrow().get(&id).cloned();
                if let Some(handle) = handle {
                    handle.set(match data {
                        Some(svg) => AssetState::Ready(svg),
                        None => AssetState::Failed,
                    });
                }
            },
            || {},
        );
        *self.task.borrow_mut() = Some(task);
    }
}

impl Drop for HttpAssetSource {
    fn drop(&mut self) {
        if let Some(task) = self.task.borrow_mut().take() {
            task.cancel();
        }
    }
}

impl AssetSource for HttpAssetSource {
    fn svg(&self, id: &str) -> ReadSignal<AssetState<Arc<SvgData>>> {
        self.ensure_worker();
        if let Some(existing) = self.store.states.borrow().get(id) {
            return existing.read_only();
        }
        let handle = signal(AssetState::Loading);
        let read = handle.read_only();
        self.store
            .states
            .borrow_mut()
            .insert(id.to_string(), handle);
        let _ = self.requests.send(id.to_string());
        read
    }
}

/// Resolves each requested id — disk cache first, then the network — and emits the parsed SVG back to the UI
/// thread. Ends when the source is dropped and the request channel closes.
///
/// Requests are served one at a time and retries wait *inside this loop* rather than on a timer, so a whole
/// screen of icons costs one thread and one connection rather than one of each per glyph, and a provider that
/// is down is retried at the configured rate instead of as fast as the pool will spawn.
fn run_worker(requests: Receiver<String>, fetch: Fetch, out: Emitter<Delivery>) {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(fetch.timeout))
        .build()
        .into();

    let mut ready: VecDeque<(String, u32)> = VecDeque::new();
    let mut retries: Vec<(String, u32, Instant)> = Vec::new();

    loop {
        if out.is_cancelled() {
            return;
        }

        let now = Instant::now();
        retries.retain(|(id, attempts, at)| {
            if *at <= now {
                ready.push_back((id.clone(), *attempts));
                false
            } else {
                true
            }
        });

        if ready.is_empty() {
            match retries.iter().map(|(_, _, at)| *at).min() {
                Some(at) => {
                    match requests.recv_timeout(at.saturating_duration_since(Instant::now())) {
                        Ok(id) => ready.push_back((id, 0)),
                        Err(mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(mpsc::RecvTimeoutError::Disconnected) => return,
                    }
                }
                None => match requests.recv() {
                    Ok(id) => ready.push_back((id, 0)),
                    Err(_) => return,
                },
            }
        }
        // Absorb the rest of a burst before fetching, so a grid of icons queues in one pass.
        while let Ok(id) = requests.try_recv() {
            ready.push_back((id, 0));
        }

        let Some((id, attempts)) = ready.pop_front() else {
            continue;
        };
        match load(&id, &fetch, &agent) {
            Some(svg) => out.emit((id, Some(svg))),
            None => {
                let attempts = attempts + 1;
                if attempts >= fetch.max_attempts {
                    tracing::warn!(
                        "asset '{id}' gave up after {attempts} attempts; check the name and the endpoint"
                    );
                    out.emit((id, None));
                } else {
                    retries.push((id, attempts, Instant::now() + fetch.retry_delay));
                }
            }
        }
    }
}

fn cached(id: &str, cache_dir: Option<&PathBuf>) -> Option<Arc<SvgData>> {
    let path = cache_dir?.join(cache_file_name(id));
    let text = std::fs::read_to_string(path).ok()?;
    SvgData::from_str(&text).ok().map(Arc::new)
}

/// The disk copy if there is one, else a download. A body that does not parse as SVG is a failure and is not
/// written: a provider's 404 page is a 200 with HTML in it often enough that caching the response would make
/// one typo permanent.
fn load(id: &str, fetch: &Fetch, agent: &ureq::Agent) -> Option<Arc<SvgData>> {
    if let Some(svg) = cached(id, fetch.cache_dir.as_ref()) {
        return Some(svg);
    }
    let url = fetch.url_template.replace("{name}", id);
    let body = agent
        .get(&url)
        .call()
        .ok()?
        .body_mut()
        .read_to_string()
        .ok()?;
    let svg = SvgData::from_str(&body).ok()?;
    if let Some(dir) = fetch.cache_dir.as_ref() {
        write_cache(dir, id, &body);
    }
    Some(Arc::new(svg))
}

fn write_cache(dir: &Path, id: &str, body: &str) {
    if let Err(e) = std::fs::create_dir_all(dir) {
        tracing::warn!("could not create asset cache {}: {e}", dir.display());
        return;
    }
    let path = dir.join(cache_file_name(id));
    if let Err(e) = std::fs::write(&path, body) {
        tracing::warn!("could not cache asset {}: {e}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_id_is_its_own_file_name() {
        assert_eq!(cache_file_name("arrow-right"), "arrow-right.svg");
    }

    #[test]
    fn ids_that_differ_only_in_punctuation_do_not_share_a_file() {
        let colon = cache_file_name("mdi:home");
        let slash = cache_file_name("mdi/home");
        assert!(colon.starts_with("mdi_home_"), "{colon}");
        assert_ne!(colon, slash);
    }

    /// A separator in an id must not become a separator in the path, or the cache writes outside itself.
    #[test]
    fn an_id_cannot_escape_the_cache_directory() {
        let name = cache_file_name("../../etc/passwd");
        assert!(!name.contains('/'), "{name}");
        assert!(!name.contains(".."), "{name}");
        assert_eq!(
            Path::new("/cache").join(&name).parent().unwrap(),
            Path::new("/cache")
        );
    }

    #[test]
    fn the_template_substitutes_the_whole_id() {
        let url = "https://api.iconify.design/{name}.svg".replace("{name}", "mdi/home");
        assert_eq!(url, "https://api.iconify.design/mdi/home.svg");
    }
}
