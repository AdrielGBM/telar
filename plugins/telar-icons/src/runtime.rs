//! Resolving ids that were not baked, as the application runs.
//!
//! With the `runtime` feature an application installs a [`RuntimeIcons`] — an Iconify-compatible provider, or any [`IconSource`](crate::IconSource) — and every id the `icon` tag holds that the bake did not answer for goes to it. Without one, such an id draws an empty box and says so once in the log.

use std::sync::Arc;

use telar::SvgData;

/// The artwork for `id` if a runtime source has it now, `None` while it loads or when nothing can answer for it.
pub(crate) fn resolve(id: &str) -> Option<Arc<SvgData>> {
    #[cfg(feature = "runtime")]
    if let Some(answer) = installed::resolve(id) {
        return answer;
    }
    warn_once(id, || {
        format!(
            "icon `{id}` was not baked and no runtime source is installed, so it draws nothing. Bake it: name a source in `[telar.icons]` of telar.toml and run `cargo telar bake`; or resolve it at run time: enable the `runtime` feature of telar-icons and install a `RuntimeIcons` from `telar::app!`'s setup block"
        )
    });
    None
}

/// Logs `message` the first time `id` is the subject of one, so a frame that redraws a missing icon sixty times a second says it once.
fn warn_once(id: &str, message: impl FnOnce() -> String) {
    use std::cell::RefCell;
    use std::collections::HashSet;
    use std::mem::ManuallyDrop;

    thread_local! {
        // ManuallyDrop: a hot-reload dylib is unmapped with this thread still running, and a TLS destructor left behind would point into it.
        static WARNED: ManuallyDrop<RefCell<HashSet<String>>> = ManuallyDrop::new(RefCell::new(HashSet::new()));
    }
    let first = WARNED.with(|warned| warned.borrow_mut().insert(id.to_string()));
    if first {
        tracing::warn!("{}", message());
    }
}

#[cfg(feature = "runtime")]
pub use installed::RuntimeIcons;

#[cfg(feature = "runtime")]
mod installed {
    use std::cell::RefCell;
    use std::mem::ManuallyDrop;
    use std::sync::Arc;

    use icons_core::{IconId, IconSource};
    use telar::{
        AssetCache, AssetError, AssetKey, AssetLoader, AssetState, AssetTransport, Reply, SvgData,
    };
    use telar_dynamic::{HttpTransport, MemoryCache, SvgDecoder};

    thread_local! {
        // ManuallyDrop for the same reason as `WARNED`; the loader also owns the signals it handed out, which live on this thread.
        static LOADER: ManuallyDrop<RefCell<Option<AssetLoader<Arc<SvgData>>>>> =
            const { ManuallyDrop::new(RefCell::new(None)) };
    }

    /// Where the ids an application did not bake are resolved while it runs: a transport the icons arrive through and, by default, a memory cache that spares a second request within the run.
    ///
    /// Install one from `telar::app!`'s setup block, which runs in the application and again in every hot-reload dylib: the loader is per thread and per image, like everything a plugin installs.
    ///
    /// ```ignore
    /// telar::app!(
    ///     app::theme::AppTheme,
    ///     {
    ///         telar_icons::RuntimeIcons::provider("https://icons.example.com")
    ///             .with_cache(telar_icons::DiskCache::new(cache_dir.join("icons")))
    ///             .install();
    ///     },
    ///     telar::AppConfig::default(),
    ///     app::Root
    /// );
    /// ```
    pub struct RuntimeIcons {
        transport: Arc<dyn AssetTransport>,
        cache: Option<Arc<dyn AssetCache>>,
        max_in_flight: usize,
    }

    impl RuntimeIcons {
        /// An Iconify-compatible API at `base`, asked for `{base}/{set}/{name}.svg`: the public `https://api.iconify.design`, or the same server self-hosted. Each request tells that server which icon the application is drawing, from the user's address.
        pub fn provider(base: &str) -> Self {
            let template = format!("{}/{{name}}.svg", base.trim_end_matches('/'));
            Self::transport(HttpTransport::new(template))
        }

        /// Any icon source, asked from the task pool: an [`SvgDir`](crate::SvgDir) or [`IconifyDir`](crate::IconifyDir) the application ships or downloads, or one of its own. A source reading files does nothing in a browser, which has no filesystem to read.
        pub fn source(source: impl IconSource + 'static) -> Self {
            Self::transport(SourceTransport(Arc::new(source)))
        }

        /// Any transport, handed ids as `set/name`.
        pub fn transport(transport: impl AssetTransport) -> Self {
            Self {
                transport: Arc::new(transport),
                cache: Some(Arc::new(MemoryCache::default())),
                max_in_flight: 4,
            }
        }

        /// Keeps what arrives in `cache` instead of in memory: a `DiskCache` spares the requests across restarts, and lets an icon seen once draw offline.
        pub fn with_cache(mut self, cache: impl AssetCache) -> Self {
            self.cache = Some(Arc::new(cache));
            self
        }

        /// Asks the transport every time.
        pub fn without_cache(mut self) -> Self {
            self.cache = None;
            self
        }

        /// How many icons may be on their way at once. 4 by default.
        pub fn with_max_in_flight(mut self, max: usize) -> Self {
            self.max_in_flight = max;
            self
        }

        /// Makes this the source every `icon` on this thread resolves unbaked ids through, replacing any installed before.
        pub fn install(self) {
            let loader = AssetLoader::new(self.transport, self.cache, Arc::new(SvgDecoder))
                .with_max_in_flight(self.max_in_flight);
            LOADER.with(|slot| *slot.borrow_mut() = Some(loader));
        }

        /// Removes the installed source, if any, cancelling what it still has in flight.
        pub fn uninstall() {
            LOADER.with(|slot| slot.borrow_mut().take());
        }
    }

    /// `None` when no source is installed. Otherwise what it has for `id` now, subscribing the caller so it draws again when the icon lands.
    pub(super) fn resolve(id: &str) -> Option<Option<Arc<SvgData>>> {
        LOADER.with(|slot| {
            let slot = slot.borrow();
            let loader = slot.as_ref()?;
            let path = match IconId::parse(id) {
                Ok(id) => id.path(),
                Err(error) => {
                    super::warn_once(id, || format!("{error}; the icon draws nothing"));
                    return Some(None);
                }
            };
            Some(match loader.get(&path).get() {
                AssetState::Ready(svg) => Some(svg),
                AssetState::Loading => None,
                AssetState::Failed => {
                    super::warn_once(id, || {
                        format!(
                            "icon `{id}` could not be resolved at run time, so it draws nothing"
                        )
                    });
                    None
                }
            })
        })
    }

    /// An [`IconSource`] as an asset transport, so a loader can resolve through it.
    struct SourceTransport(Arc<dyn IconSource>);

    impl AssetTransport for SourceTransport {
        fn load(&self, key: &AssetKey, reply: Reply) {
            let answer = IconId::from_path(&key.id)
                .and_then(|id| self.0.icon(&id))
                .map_err(|error| AssetError(error.to_string()))
                .and_then(|icon| {
                    icon.map(|icon| icon.svg.into_bytes()).ok_or_else(|| {
                        AssetError(format!("no icon `{}` in {}", key.id, self.0.describe()))
                    })
                });
            reply(answer);
        }
    }
}
