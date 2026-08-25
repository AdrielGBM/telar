//! The caches every surface draws from, held once per thread rather than once per renderer.
//!
//! What belongs here is decided by the key, not by the size: a cache addressed by *content* — a glyph, an image,
//! a shadow's geometry — answers the same question for every surface, so one copy serves all of them. State
//! addressed by *surface* — the framebuffer, the clip mask, the previous frame's commands — stays on the
//! renderer, where it belongs.
//!
//! Thread-local rather than behind a lock because a [`crate::SoftwareRenderer`] is already thread-bound (it owns
//! a softbuffer `Surface`, which is not `Send`), so a mutex would buy nothing but contention on the frame path.
//! An app that renders surfaces on several threads gets one set per thread — no sharing across them, but no
//! worse than a set per renderer either.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::mpsc;

use renderer_cache::{Cache, CacheStat};
use renderer_text::{TextShaper, TextShaperConfig};
use tiny_skia::Pixmap;

use crate::SoftwareRendererConfig;
use crate::primitives::image::ShadowCacheKey;
use crate::primitives::path::PathShadowCacheKey;
use crate::primitives::text::TextShadowCacheKey;

/// What one cached pixmap costs. The whole reason these caches are bounded by bytes and not by count: a 4K image is
/// ~33 MiB, so an entry cap would let any of them grow into the gigabytes.
pub(crate) fn pixmap_bytes(pixmap: &Pixmap) -> usize {
    pixmap.data().len()
}

pub(crate) struct SharedCaches {
    pub(crate) text_shaper: TextShaper,
    pub(crate) shadow_cache: Cache<ShadowCacheKey, Pixmap>,
    pub(crate) text_shadow_cache: Cache<TextShadowCacheKey, Pixmap>,
    pub(crate) path_shadow_cache: Cache<PathShadowCacheKey, Pixmap>,
    // In-flight background shadow work. Kept beside the cache each one drains into: a worker spawned for a surface produces a pixmap keyed by geometry alone, so whichever surface asks next should get it.
    pub(crate) pending_shadows: HashMap<ShadowCacheKey, mpsc::Receiver<Pixmap>>,
    pub(crate) pending_text_shadows: HashMap<TextShadowCacheKey, mpsc::Receiver<Pixmap>>,
    pub(crate) pending_path_shadows: HashMap<PathShadowCacheKey, mpsc::Receiver<Pixmap>>,
    // The last shadow of each kind actually drawn, with the size it was drawn at, so a re-keyed one can stand in with it while its blur is in flight instead of leaving a hole (see `blit_cached_shadow_async`).
    pub(crate) recent_shadow: Option<(ShadowCacheKey, u32, u32)>,
    pub(crate) recent_text_shadow: Option<(TextShadowCacheKey, u32, u32)>,
    pub(crate) recent_path_shadow: Option<(PathShadowCacheKey, u32, u32)>,
}

impl SharedCaches {
    fn new(config: &SoftwareRendererConfig) -> Self {
        Self {
            text_shaper: TextShaper::with_config(TextShaperConfig {
                raster: renderer_cache::limits::TEXT_RASTER,
                shaping: renderer_cache::limits::TEXT_SHAPING,
                font: config.font.clone(),
            }),
            shadow_cache: Cache::new(renderer_cache::limits::SHADOW, pixmap_bytes),
            text_shadow_cache: Cache::new(renderer_cache::limits::TEXT_SHADOW, pixmap_bytes),
            path_shadow_cache: Cache::new(renderer_cache::limits::PATH_SHADOW, pixmap_bytes),
            pending_shadows: HashMap::new(),
            pending_text_shadows: HashMap::new(),
            pending_path_shadows: HashMap::new(),
            recent_shadow: None,
            recent_text_shadow: None,
            recent_path_shadow: None,
        }
    }

    fn stats(&self) -> Vec<CacheStat> {
        let mut stats = self.text_shaper.cache_stats();
        stats.push(self.shadow_cache.stat("shadow.rect"));
        stats.push(self.text_shadow_cache.stat("shadow.text"));
        stats.push(self.path_shadow_cache.stat("shadow.path"));
        stats
    }
}

thread_local! {
    static CACHES: RefCell<Option<SharedCaches>> = const { RefCell::new(None) };
}

/// Builds this thread's caches if no renderer has yet, using `config`'s budgets and fonts.
///
/// Called by every [`crate::SoftwareRenderer`] before its first frame — from `bind_to_render_thread` when a render thread will drive it, and from `begin_frame` otherwise. Deliberately *not* from the constructors: these are thread-local, and an on-screen renderer is built on the UI thread but draws on its own, so seeding them at construction would furnish the wrong thread. Later surfaces sharing the thread do not resize them: budgets that grew per surface are the thing this module exists to stop, and a shared cache sized by whoever drew first is the predictable choice. Fonts are already process-wide — one database behind every shaper, see `renderer_text::fonts` — so a second surface asking for different ones is not a case telar produces.
pub(crate) fn init(config: &SoftwareRendererConfig) {
    CACHES.with_borrow_mut(|slot| {
        slot.get_or_insert_with(|| SharedCaches::new(config));
    });
}

/// Whether this thread has built its caches yet. Lets a test tell "the constructor seeded the wrong thread"
/// apart from "the drawing thread seeded itself", which is the whole point of deferring [`init`].
#[cfg(test)]
pub(crate) fn initialised() -> bool {
    CACHES.with_borrow(|slot| slot.is_some())
}

/// Drops everything no frame has asked for within each cache's idle horizon.
///
/// The caches sweep themselves as they are used, which is enough while frames keep coming. It is not enough for a
/// shell that has drawn nothing for an hour: with no accesses there is no sweep, and the high-water mark stands.
///
/// Sweeps **the calling thread's** caches, since that is where they live. An on-screen surface's caches belong to
/// its render thread, which sweeps itself once it has been idle (`RenderBackend::sweep_idle_caches`) — calling
/// this from the UI thread would not reach them. What it is for is the caches a thread built by rasterising
/// directly: `telar::rasterize`, previews, offscreen renderers.
pub fn sweep_idle() {
    with_caches(|c| {
        c.text_shaper.sweep_idle();
        c.shadow_cache.sweep();
        c.text_shadow_cache.sweep();
        c.path_shadow_cache.sweep();
    });
}

/// Leaves this thread's cache census where another thread can read it. Called once per frame, throttled.
pub(crate) fn publish_stats() {
    if renderer_cache::registry::publish_due() {
        let stats = with_caches(|c| c.stats());
        renderer_cache::registry::publish(stats);
    }
}

/// Opens this thread's caches, building them from defaults if no renderer did.
///
/// Destructure the `&mut SharedCaches` when a call needs two of them at once — a `retain` over one pending map writing into its cache, say. Field-by-field borrows are disjoint; the whole struct is not.
pub(crate) fn with_caches<R>(f: impl FnOnce(&mut SharedCaches) -> R) -> R {
    CACHES.with_borrow_mut(|slot| {
        f(slot.get_or_insert_with(|| SharedCaches::new(&SoftwareRendererConfig::default())))
    })
}
