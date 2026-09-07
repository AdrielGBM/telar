//! The reactive seam for assets that resolve later, and the transport-agnostic vocabulary — a key, an error, and the transport/cache/decoder roles — that an implementor assembles into one.
//!
//! [`AssetTransport`], [`AssetCache`] and [`AssetDecoder`] are three roles, not one: bytes in, bytes kept, bytes decoded, with no method to add anywhere for a format nobody asked for. [`AssetLoader`] assembles them into the `Loading` → `Ready`/`Failed` contract a widget reads. This crate ships no implementation of any of the three — `telar-dynamic` carries ours, and an application's own arrives through exactly the same door.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::sync::Arc;

use reactive_core::{Emitter, ReadSignal, RwSignal, Task, signal, spawn_stream};

/// The lifecycle of an asynchronously-resolved asset. Kept inside a signal so a widget re-renders as the state advances from `Loading` to `Ready`/`Failed`.
#[derive(Clone, Debug)]
pub enum AssetState<T> {
    Loading,
    Ready(T),
    Failed,
}

/// Two resolved states are the same asset when they hold the same `Arc` — identity, not contents. Comparing a decoded asset structurally would walk every path of every glyph to answer a question the pointer already answers, and answer it differently: two identical decodes of the same file are still two arrivals.
///
/// Only `Arc` payloads, which is what a decoder of a shared, heap-sized asset hands out. It exists so a `memo` can hold one, which is what lets a view `match` on an asset as it resolves.
impl<T> PartialEq for AssetState<std::sync::Arc<T>> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (AssetState::Loading, AssetState::Loading) => true,
            (AssetState::Failed, AssetState::Failed) => true,
            (AssetState::Ready(a), AssetState::Ready(b)) => std::sync::Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

/// The `&'static` counterpart of the `Arc` impl above, for a payload that is leaked rather than reference-counted — a translation catalog, say. Same identity comparison, so it can live in a `memo` too.
impl<T> PartialEq for AssetState<&'static T> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (AssetState::Loading, AssetState::Loading) => true,
            (AssetState::Failed, AssetState::Failed) => true,
            (AssetState::Ready(a), AssetState::Ready(b)) => std::ptr::eq(*a, *b),
            _ => false,
        }
    }
}

impl<T> AssetState<T> {
    pub fn is_ready(&self) -> bool {
        matches!(self, AssetState::Ready(_))
    }

    pub fn as_ready(&self) -> Option<&T> {
        match self {
            AssetState::Ready(value) => Some(value),
            _ => None,
        }
    }

    pub fn into_ready(self) -> Option<T> {
        match self {
            AssetState::Ready(value) => Some(value),
            _ => None,
        }
    }
}

/// What names one asset across transport, cache and decoder: the id an application or `.rsx` file writes (`"mdi/home"`, `"en-US"`), plus which decoder resolves it.
///
/// `kind` is what keeps two different resources from colliding on the same id — an SVG `"logo"` and a bitmap `"logo"` are different assets, and a cache keyed on `id` alone would let one overwrite the other's file. It comes from [`AssetDecoder::kind`], so it is the loader's job to fill in, not the id's.
///
/// `id` is owned rather than borrowed: a transport's own retry queue, and a cache's own lookup, both outlive the single call that hands them a `&AssetKey`, so the key they keep has to be able to outlive it too.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AssetKey {
    pub kind: &'static str,
    pub id: String,
}

impl AssetKey {
    pub fn new(kind: &'static str, id: impl Into<String>) -> Self {
        Self {
            kind,
            id: id.into(),
        }
    }
}

/// Why an asset failed to resolve — the transport could not get the bytes, or the decoder could not make sense of them. Carries a message for logs and diagnostics; nothing in the seam matches on it structurally.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetError(pub String);

impl std::fmt::Display for AssetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for AssetError {}

/// Delivers the bytes a transport fetched, or why it could not. Boxed rather than typed as a future: a job queued on `reactive_core`'s task pool runs synchronously to completion on the web target (see [`AssetTransport`]), so there is no executor here to poll one.
pub type Reply = Box<dyn FnOnce(Result<Vec<u8>, AssetError>) + Send + 'static>;

/// Fetches the raw bytes behind an [`AssetKey`] — from disk, from the network, from a bundle — with no opinion on what they decode to. Dyn-safe on purpose: one transport is shared across every loader that reads through it, whatever each one decodes its bytes into.
pub trait AssetTransport: Send + Sync + 'static {
    /// Starts resolving `key` and calls `reply` exactly once with the result, from whatever thread ends up holding it.
    ///
    /// Runs inside a job on `reactive_core`'s task pool: a worker thread natively, but the browser's own event loop on the web target, where a job that blocks freezes the page. A transport that waits on the network or the disk has to reach it asynchronously there — starting the request and moving `reply` into its completion callback — rather than parking the calling thread.
    fn load(&self, key: &AssetKey, reply: Reply);
}

/// Keeps bytes already resolved once so a later request for the same [`AssetKey`] does not repeat the transport. Dyn-safe on purpose: one cache is shared across every transport that reads through it.
pub trait AssetCache: Send + Sync + 'static {
    /// The cached bytes for `key`, if any were kept.
    fn get(&self, key: &AssetKey) -> Option<Vec<u8>>;

    /// Keeps `bytes` under `key` for a later [`get`](Self::get). A cache is not obligated to call this on every write it is offered — a loader only reaches it once bytes have already decoded successfully, so nothing here needs to re-derive that a 404 page is not a valid asset.
    fn put(&self, key: &AssetKey, bytes: &[u8]);
}

/// Turns bytes into the handle a widget actually holds — an `Arc<SvgData>`, a `&'static Catalog` — for exactly one resource type.
///
/// Stored as `Arc<dyn AssetDecoder<Output = V>>`: fixing `Output` is what keeps the trait dyn-safe despite being generic over it. `kind` is a method rather than the associated const the shape might suggest, because an associated const has no vtable slot and blocks object safety outright — a `dyn AssetDecoder` could never report one.
pub trait AssetDecoder: Send + Sync + 'static {
    /// Names this resource type for cache namespacing ([`AssetKey::kind`]) and in error messages. Constant in practice — an implementor typically returns a literal — but expressed as a method so it can be read through the trait object the loader actually holds.
    fn kind(&self) -> &'static str;

    /// The decoded handle a widget holds onto, not the bytes it was made from: `Clone` because a signal hands out copies of it, `Send` because decoding happens off the UI thread.
    type Output: Clone + Send + 'static;

    fn decode(&self, bytes: &[u8]) -> Result<Self::Output, AssetError>;
}

/// How many requests an [`AssetLoader`] lets sit open against its transport before the rest wait in line.
const DEFAULT_MAX_IN_FLIGHT: usize = 1;

/// The state one [`AssetLoader`] shares with every request it has in flight, kept behind an `Rc` so a request's completion callback can reach back into it without having to keep the loader itself alive.
struct Inner<V: Clone + Send + 'static> {
    transport: Arc<dyn AssetTransport>,
    cache: Option<Arc<dyn AssetCache>>,
    decoder: Arc<dyn AssetDecoder<Output = V>>,
    states: RefCell<HashMap<String, RwSignal<AssetState<V>>>>,
    queue: RefCell<VecDeque<String>>,
    in_flight: Cell<usize>,
    max_in_flight: Cell<usize>,
    tasks: RefCell<HashMap<String, Task>>,
}

impl<V: Clone + Send + 'static> Inner<V> {
    fn enqueue(self: &Rc<Self>, id: String) {
        if self.in_flight.get() < self.max_in_flight.get() {
            self.start(id);
        } else {
            self.queue.borrow_mut().push_back(id);
        }
    }

    /// Resolves `id` through the cache, then the transport, and writes whichever lands first into its signal.
    ///
    /// One job on `reactive_core`'s task pool per request rather than a worker that loops waiting on the next one: a native target runs it on a pooled thread that retires when idle, a web target queues it as a microtask, and neither is asked to park on a channel the way a perpetual worker's `recv_timeout` would.
    fn start(self: &Rc<Self>, id: String) {
        self.in_flight.set(self.in_flight.get() + 1);
        let key = AssetKey::new(self.decoder.kind(), id.clone());

        let cache = self.cache.clone();
        let transport = Arc::clone(&self.transport);
        let decoder = Arc::clone(&self.decoder);

        let store = Rc::clone(self);
        let item_id = id.clone();
        let end_store = Rc::clone(self);
        let end_id = id.clone();

        // `on_end` fires once every clone of `out` handed to `transport.load` has dropped — immediately for a cache hit, or whenever the transport's own `reply` eventually runs, sync or not. That is what lets one job stand for the whole request instead of just the part that starts it.
        let task = spawn_stream(
            move |out: Emitter<Result<V, AssetError>>| {
                if let Some(bytes) = cache.as_deref().and_then(|c| c.get(&key))
                    && let Ok(value) = decoder.decode(&bytes)
                {
                    out.emit(Ok(value));
                    return;
                }
                let out_reply = out.clone();
                let decoder_reply = Arc::clone(&decoder);
                let cache_reply = cache.clone();
                let reply_key = key.clone();
                transport.load(
                    &key,
                    Box::new(move |result| {
                        out_reply.emit(decode_and_cache(
                            result,
                            decoder_reply.as_ref(),
                            cache_reply.as_deref(),
                            &reply_key,
                        ));
                    }),
                );
            },
            move |outcome: Result<V, AssetError>| {
                if let Some(signal) = store.states.borrow().get(&item_id) {
                    signal.set(match outcome {
                        Ok(value) => AssetState::Ready(value),
                        Err(_) => AssetState::Failed,
                    });
                }
            },
            move || {
                end_store.tasks.borrow_mut().remove(&end_id);
                end_store.in_flight.set(end_store.in_flight.get() - 1);
                if let Some(next) = end_store.queue.borrow_mut().pop_front() {
                    end_store.start(next);
                }
            },
        );

        self.tasks.borrow_mut().insert(id, task);
    }
}

/// The cache write sits behind the same `?` that already proved the bytes decoded, so a failed decode cannot reach [`AssetCache::put`] by construction — not by a caller remembering to check first, the way the worker this replaces relied on an `if` for the same guarantee.
fn decode_and_cache<V: Clone + Send + 'static>(
    result: Result<Vec<u8>, AssetError>,
    decoder: &dyn AssetDecoder<Output = V>,
    cache: Option<&dyn AssetCache>,
    key: &AssetKey,
) -> Result<V, AssetError> {
    let bytes = result?;
    let value = decoder.decode(&bytes)?;
    if let Some(cache) = cache {
        cache.put(key, &bytes);
    }
    Ok(value)
}

/// Resolves ids into `V` through a cache-then-transport pipeline, reactively: [`get`](Self::get) returns at once with a signal that advances `Loading` → `Ready`/`Failed` as the answer arrives.
///
/// A concrete type rather than a trait: nothing here needs to be dyn-safe, so [`get_or`](Self::get_or) takes its fallback as a plain `impl FnOnce` — a shape a trait with this method could not have had at all. What varies is behind it, in the three roles it composes. One loader serves one [`AssetDecoder::Output`] — an application with both SVGs and bitmaps builds two, sharing a transport and a cache between them if it wants to.
///
/// Every read for an id already loading or loaded shares the request that started it, so a screen full of the same icon costs one transport call, not one per widget that asked. Requests beyond [`with_max_in_flight`](Self::with_max_in_flight) — 1 by default — queue and start as earlier ones finish, so the loader throttles concurrency itself rather than leaving that to the transport.
///
/// `!Send` by construction: it holds the signals it hands out, and those live on the UI thread. Construct it there and keep it there — a `thread_local!`, or a field of the app. Dropping it cancels every request still in flight, so nothing it started can write into a signal nobody can read anymore.
pub struct AssetLoader<V: Clone + Send + 'static> {
    inner: Rc<Inner<V>>,
}

impl<V: Clone + Send + 'static> AssetLoader<V> {
    /// A loader over `transport`, decoding through `decoder` and, when given, keeping resolved bytes in `cache`.
    pub fn new(
        transport: Arc<dyn AssetTransport>,
        cache: Option<Arc<dyn AssetCache>>,
        decoder: Arc<dyn AssetDecoder<Output = V>>,
    ) -> Self {
        Self {
            inner: Rc::new(Inner {
                transport,
                cache,
                decoder,
                states: RefCell::new(HashMap::new()),
                queue: RefCell::new(VecDeque::new()),
                in_flight: Cell::new(0),
                max_in_flight: Cell::new(DEFAULT_MAX_IN_FLIGHT),
                tasks: RefCell::new(HashMap::new()),
            }),
        }
    }

    /// How many requests may be open against the transport at once; the rest wait their turn. Clamped to at least 1.
    pub fn with_max_in_flight(self, max: usize) -> Self {
        self.inner.max_in_flight.set(max.max(1));
        self
    }

    /// A reactive handle to the asset named `id`. Reading it subscribes the caller and, on first ask, starts resolving it; a later call for the same id returns the same signal instead of asking the transport again.
    pub fn get(&self, id: &str) -> ReadSignal<AssetState<V>> {
        if let Some(existing) = self.inner.states.borrow().get(id) {
            return existing.read_only();
        }
        let handle = signal(AssetState::Loading);
        let read = handle.read_only();
        self.inner
            .states
            .borrow_mut()
            .insert(id.to_string(), handle);
        self.inner.enqueue(id.to_string());
        read
    }

    /// The resolved asset, or `fallback()` while it is still loading or has failed. Reads reactively, so a widget calling this re-renders once the asset lands.
    pub fn get_or(&self, id: &str, fallback: impl FnOnce() -> V) -> V {
        match self.get(id).get() {
            AssetState::Ready(value) => value,
            _ => fallback(),
        }
    }
}

impl<V: Clone + Send + 'static> Drop for AssetLoader<V> {
    fn drop(&mut self) {
        for (_, task) in self.inner.tasks.borrow_mut().drain() {
            task.cancel();
        }
    }
}

#[cfg(test)]
#[path = "async_asset_test.rs"]
mod tests;

#[cfg(test)]
#[path = "async_asset_loader_test.rs"]
mod loader_tests;
