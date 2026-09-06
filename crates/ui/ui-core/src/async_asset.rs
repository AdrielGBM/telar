//! The reactive seam for assets that resolve later, and the transport-agnostic vocabulary — a key, an error, and the transport/cache/decoder roles — that an implementor assembles into one.
//!
//! [`AssetState`] and [`AssetSource`] are the older, still-supported shape: one method per asset format, with transport, caching and decoding folded together inside a single implementor like `HttpAssetSource`. [`AssetTransport`], [`AssetCache`] and [`AssetDecoder`] are the shape a new resource type should target instead — bytes in, bytes cached, bytes decoded, with no method to add anywhere for a format nobody asked for. A future loader assembles the three into the same `Loading` → `Ready`/`Failed` contract `AssetSource` exposes today.

use std::sync::Arc;

use reactive_core::ReadSignal;
use renderer_assets::SvgData;

/// The lifecycle of an asynchronously-resolved asset. Kept inside a signal so a widget re-renders as the state advances from `Loading` to `Ready`/`Failed`.
#[derive(Clone, Debug)]
pub enum AssetState<T> {
    Loading,
    Ready(T),
    Failed,
}

/// Two resolved states are the same asset when they hold the same `Arc` — identity, not contents. Comparing a decoded asset structurally would walk every path of every glyph to answer a question the pointer already answers, and answer it differently: two identical decodes of the same file are still two arrivals.
///
/// Only `Arc` payloads, which is what an [`AssetSource`] hands out. It exists so a `memo` can hold one, which is what lets a view `match` on an asset as it resolves.
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

/// A transport-agnostic source of SVG assets addressed by string id. rsx owns the reactive contract — a signal that advances `Loading` → `Ready`/`Failed` and re-renders whoever read it — while an implementor supplies the bytes from disk, a bundle, or the network entirely outside this crate.
pub trait AssetSource {
    /// A reactive handle to the SVG named `id`. Reading it subscribes the caller and may start the load; the state advances as the asset resolves.
    fn svg(&self, id: &str) -> ReadSignal<AssetState<Arc<SvgData>>>;

    /// The resolved SVG, or `fallback()` while it is still loading or has failed. Reads reactively, so a widget calling this re-renders once the asset lands.
    fn svg_or(&self, id: &str, fallback: impl FnOnce() -> Arc<SvgData>) -> Arc<SvgData> {
        match self.svg(id).get() {
            AssetState::Ready(data) => data,
            _ => fallback(),
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

#[cfg(test)]
mod tests {
    use super::*;

    struct NullTransport;
    impl AssetTransport for NullTransport {
        fn load(&self, _key: &AssetKey, reply: Reply) {
            reply(Err(AssetError("no transport in a test".into())));
        }
    }

    struct MemCache;
    impl AssetCache for MemCache {
        fn get(&self, _key: &AssetKey) -> Option<Vec<u8>> {
            None
        }
        fn put(&self, _key: &AssetKey, _bytes: &[u8]) {}
    }

    struct UpperDecoder;
    impl AssetDecoder for UpperDecoder {
        fn kind(&self) -> &'static str {
            "upper"
        }
        type Output = Arc<str>;
        fn decode(&self, bytes: &[u8]) -> Result<Self::Output, AssetError> {
            let text = std::str::from_utf8(bytes).map_err(|e| AssetError(e.to_string()))?;
            Ok(Arc::from(text.to_uppercase()))
        }
    }

    /// The property the trait split is built on: a loader shared across resource types needs to hold these behind `Arc<dyn _>`, and `AssetDecoder` in particular loses that if its `kind` ever goes back to being an associated const instead of a method.
    #[test]
    fn transport_cache_and_decoder_are_all_object_safe() {
        let transport: Arc<dyn AssetTransport> = Arc::new(NullTransport);
        let cache: Arc<dyn AssetCache> = Arc::new(MemCache);
        let decoder: Arc<dyn AssetDecoder<Output = Arc<str>>> = Arc::new(UpperDecoder);

        let key = AssetKey::new(decoder.kind(), "greeting");
        assert!(cache.get(&key).is_none());
        cache.put(&key, b"hi");

        transport.load(&key, Box::new(|result| assert!(result.is_err())));
        assert_eq!(&*decoder.decode(b"hi").unwrap(), "HI");
    }

    #[test]
    fn a_key_carries_the_kind_alongside_the_id_so_two_formats_cannot_collide() {
        let svg = AssetKey::new("svg", "logo");
        let image = AssetKey::new("image", "logo");
        assert_ne!(svg, image);
    }

    #[test]
    fn a_leaked_ready_value_compares_by_identity_like_an_arc_does() {
        #[derive(Debug)]
        struct Catalog(&'static str);

        let a: &'static Catalog = Box::leak(Box::new(Catalog("catalog")));
        let b: &'static Catalog = Box::leak(Box::new(Catalog("catalog")));
        assert_eq!(AssetState::Ready(a), AssetState::Ready(a));
        assert_ne!(AssetState::Ready(a), AssetState::Ready(b));
        assert_eq!(a.0, "catalog");
        assert_eq!(
            AssetState::<&'static Catalog>::Loading,
            AssetState::<&'static Catalog>::Loading
        );
    }
}
