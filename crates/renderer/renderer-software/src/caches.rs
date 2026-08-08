//! The caches every surface draws from, held once per thread rather than once per renderer.
//!
//! What belongs here is decided by the key, not by the size: a cache addressed by *content* — a glyph, an image,
//! a shadow's geometry — answers the same question for every surface, so one copy serves all of them. State
//! addressed by *surface* — the framebuffer, the clip mask, the previous frame's commands — stays on the
//! renderer, where it belongs.
//!
//! Per renderer, these cost what a surface never recovers. [`renderer_text::TextShaper`] alone opens a 2048×2048
//! RGBA glyph atlas eagerly, 16 MiB zeroed before a single glyph is drawn; a shell with six surfaces paid that
//! six times over for six atlases holding the same few hundred glyphs. Shared, the second surface to want a
//! glyph the first already rasterized gets it for nothing.
//!
//! Thread-local rather than behind a lock because a [`crate::SoftwareRenderer`] is already thread-bound (it owns
//! a softbuffer `Surface`, which is not `Send`), so a mutex would buy nothing but contention on the frame path.
//! An app that renders surfaces on several threads gets one set per thread — no sharing across them, but no
//! worse than a set per renderer either.

use std::cell::RefCell;
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::mpsc;

use renderer_text::{TextShaper, TextShaperConfig};
use tiny_skia::Pixmap;

use crate::SoftwareRendererConfig;
use crate::primitives::image::{ImageCache, ShadowCache, ShadowCacheKey, new_image_cache};
use crate::primitives::path::{PathShadowCache, PathShadowCacheKey, new_path_shadow_cache};
use crate::primitives::text::{TextShadowCache, TextShadowCacheKey, new_text_shadow_cache};

pub(crate) struct SharedCaches {
    pub(crate) text_shaper: TextShaper,
    pub(crate) image_cache: ImageCache,
    pub(crate) shadow_cache: ShadowCache,
    pub(crate) text_pixmap_cache: lru::LruCache<renderer_text::TextCacheKey, Pixmap>,
    pub(crate) text_shadow_cache: TextShadowCache,
    pub(crate) path_shadow_cache: PathShadowCache,
    // In-flight background shadow work. Kept beside the cache each one drains into: a worker spawned for a surface produces a pixmap keyed by geometry alone, so whichever surface asks next should get it.
    pub(crate) pending_shadows: HashMap<ShadowCacheKey, mpsc::Receiver<Pixmap>>,
    pub(crate) pending_text_shadows: HashMap<TextShadowCacheKey, mpsc::Receiver<Pixmap>>,
    pub(crate) pending_path_shadows: HashMap<PathShadowCacheKey, mpsc::Receiver<Pixmap>>,
}

impl SharedCaches {
    fn new(config: &SoftwareRendererConfig) -> Self {
        Self {
            text_shaper: TextShaper::with_config(TextShaperConfig {
                pixel_cache_budget_bytes: config.text_pixel_cache_bytes,
                alpha_cache_budget_bytes: config.text_alpha_cache_bytes,
                shaping_cache_budget_bytes: config.text_shaping_cache_bytes,
                font: config.font.clone(),
            }),
            image_cache: new_image_cache(config.image_cache_bytes.max(1)),
            shadow_cache: crate::primitives::image::new_shadow_cache(config.shadow_cache_bytes),
            text_pixmap_cache: lru::LruCache::new(
                NonZeroUsize::new(config.text_pixmap_cache_entries).unwrap(),
            ),
            text_shadow_cache: new_text_shadow_cache(config.text_shadow_cache_bytes),
            path_shadow_cache: new_path_shadow_cache(config.path_shadow_cache_bytes),
            pending_shadows: HashMap::new(),
            pending_text_shadows: HashMap::new(),
            pending_path_shadows: HashMap::new(),
        }
    }
}

thread_local! {
    static CACHES: RefCell<Option<SharedCaches>> = const { RefCell::new(None) };
}

/// Builds this thread's caches if no renderer has yet, using `config`'s budgets and fonts.
///
/// Called from every [`crate::SoftwareRenderer`] constructor, so by the time a frame runs the caches exist and carry the first surface's settings. Later surfaces do not resize them: budgets that grew per surface are the thing this module exists to stop, and a shared cache sized by whoever happened to be built first is the predictable choice. Fonts are already process-wide in the runtime (see `set_measure_font_config`), so a second surface asking for different ones is not a case telar produces.
pub(crate) fn init(config: &SoftwareRendererConfig) {
    CACHES.with_borrow_mut(|slot| {
        slot.get_or_insert_with(|| SharedCaches::new(config));
    });
}

/// Opens this thread's caches, building them from defaults if no renderer did.
///
/// Destructure the `&mut SharedCaches` when a call needs two of them at once — a `retain` over one pending map writing into its cache, say. Field-by-field borrows are disjoint; the whole struct is not.
pub(crate) fn with_caches<R>(f: impl FnOnce(&mut SharedCaches) -> R) -> R {
    CACHES.with_borrow_mut(|slot| {
        f(slot.get_or_insert_with(|| SharedCaches::new(&SoftwareRendererConfig::default())))
    })
}
