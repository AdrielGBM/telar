//! Resolving ids that were not baked, as the application runs.
//!
//! With the `runtime` feature an application installs a [`RuntimeIcons`] — an Iconify-compatible provider, or any [`IconSource`](crate::IconSource) — and every id the `icon` tag holds that the bake did not answer for goes to it. Without one, such an id draws an empty box and says so once in the log.

use crate::icon::Glyph;

/// The artwork for `id` if a runtime source has it now, `None` while it loads or when nothing can answer for it.
pub(crate) fn resolve(id: &str) -> Option<Glyph> {
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

    use icons_core::{IconError, IconId, IconSource};
    use telar::{
        AssetCache, AssetDecoder, AssetError, AssetKey, AssetLoader, AssetState, AssetTransport,
        Reply,
    };
    use telar_dynamic::{HttpTransport, MemoryCache, SvgDecoder};

    use crate::icon::Glyph;

    /// Written before an icon's SVG by [`SourceTransport`], which knows the icon's set, so the tint decision survives a transport and a cache that only move bytes. An XML comment, so the bytes stay an SVG document any reader takes.
    const MONOCHROME_MARK: &[u8] = b"<!--telar-icons:monochrome-->";
    const PALETTE_MARK: &[u8] = b"<!--telar-icons:palette-->";

    struct Installed {
        loader: AssetLoader<Arc<Glyph>>,
        default_set: Option<String>,
    }

    thread_local! {
        // ManuallyDrop for the same reason as `WARNED`; the loader also owns the signals it handed out, which live on this thread.
        static INSTALLED: ManuallyDrop<RefCell<Option<Installed>>> =
            const { ManuallyDrop::new(RefCell::new(None)) };
    }

    /// Where the ids an application did not bake are resolved while it runs: a transport the icons arrive through, by default a memory cache that spares a second request within the run, and the set a bare name is read in.
    ///
    /// Install one from `telar::app!`'s setup block, which runs in the application and again in every hot-reload dylib: the loader is per thread and per image, like everything a plugin installs.
    ///
    /// ```ignore
    /// telar::app!(
    ///     app::theme::AppTheme,
    ///     {
    ///         telar_icons::RuntimeIcons::provider("https://icons.example.com")
    ///             .with_default_set("mdi")
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
        default_set: Option<String>,
    }

    impl RuntimeIcons {
        /// An Iconify-compatible API at `base`, asked for `{base}/{set}/{name}.svg`: the public `https://api.iconify.design`, or the same server self-hosted. Each request tells that server which icon the application is drawing, from the user's address.
        ///
        /// The SVG endpoint says nothing about the icon's set, so an icon from a provider is tinted when it paints in `currentColor`, which is how Iconify writes every icon of a monochrome set.
        pub fn provider(base: &str) -> Self {
            let template = format!("{}/{{name}}.svg", base.trim_end_matches('/'));
            Self::transport(HttpTransport::new(template))
        }

        /// Any icon source, asked from the task pool: an [`SvgDir`](crate::SvgDir) or [`IconifyDir`](crate::IconifyDir) the application ships or downloads, or one of its own. A source reading files does nothing in a browser, which has no filesystem to read. Whether an icon is tinted is decided from what the source says about its set, as the bake decides it.
        pub fn source(source: impl IconSource + 'static) -> Self {
            Self::transport(SourceTransport(Arc::new(source)))
        }

        /// Any transport, handed ids as `set/name`. An icon it answers is tinted when its SVG paints in `currentColor`.
        pub fn transport(transport: impl AssetTransport) -> Self {
            Self {
                transport: Arc::new(transport),
                cache: Some(Arc::new(MemoryCache::default())),
                max_in_flight: 4,
                default_set: None,
            }
        }

        /// Reads a bare name in `set`, so an id `home` resolves as `mdi:home` where `set` is `"mdi"`.
        ///
        /// Give it the set `[telar.icons] default_set` names. The transpiler reads every bare name a `.rsx` writes literally in that set at build time, but `telar.toml` is not there to read while the application runs on every target, so an id that arrives at run time — from a signal, a config file — is read in the set given here. A `set` outside Iconify naming is logged and ignored, so a configuration file naming a bad one leaves bare names unresolved rather than stopping the application.
        pub fn with_default_set(mut self, set: &str) -> Self {
            self.default_set = IconId::is_valid_set(set).then(|| set.to_string());
            if self.default_set.is_none() {
                tracing::warn!(
                    "`{set}` is not an icon set prefix, which is lowercase letters and digits joined by single hyphens, like `mdi`: a bare icon name has no default set to be read in"
                );
            }
            self
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
            let loader = AssetLoader::new(self.transport, self.cache, Arc::new(GlyphDecoder))
                .with_max_in_flight(self.max_in_flight);
            let installed = Installed {
                loader,
                default_set: self.default_set,
            };
            INSTALLED.with(|slot| *slot.borrow_mut() = Some(installed));
        }

        /// Removes the installed source, if any, cancelling what it still has in flight.
        pub fn uninstall() {
            INSTALLED.with(|slot| slot.borrow_mut().take());
        }
    }

    /// `None` when no source is installed. Otherwise what it has for `id` now, subscribing the caller so it draws again when the icon lands.
    pub(super) fn resolve(id: &str) -> Option<Option<Glyph>> {
        INSTALLED.with(|slot| {
            let slot = slot.borrow();
            let installed = slot.as_ref()?;
            let path = match IconId::parse_with_default(id, installed.default_set.as_deref()) {
                Ok(id) => id.path(),
                Err(IconError::MissingSet { .. }) => {
                    super::warn_once(id, || {
                        format!(
                            "icon `{id}` names no set and the installed `RuntimeIcons` has no default set, so it draws nothing: write it as `set:name`, or read bare names in a set with `RuntimeIcons::with_default_set`"
                        )
                    });
                    return Some(None);
                }
                Err(error) => {
                    super::warn_once(id, || format!("{error}; the icon draws nothing"));
                    return Some(None);
                }
            };
            Some(match installed.loader.get(&path).get() {
                AssetState::Ready(glyph) => Some((*glyph).clone()),
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

    /// Decodes an icon's SVG and reads whether it is tinted: from the mark [`SourceTransport`] wrote, or, for bytes from any other transport, from whether it paints in `currentColor`.
    struct GlyphDecoder;

    impl AssetDecoder for GlyphDecoder {
        fn kind(&self) -> &'static str {
            "icon"
        }

        type Output = Arc<Glyph>;

        fn decode(&self, bytes: &[u8]) -> Result<Self::Output, AssetError> {
            let (monochrome, svg) = if let Some(svg) = bytes.strip_prefix(MONOCHROME_MARK) {
                (true, svg)
            } else if let Some(svg) = bytes.strip_prefix(PALETTE_MARK) {
                (false, svg)
            } else {
                (
                    icons_core::uses_current_color(&String::from_utf8_lossy(bytes)),
                    bytes,
                )
            };
            Ok(Arc::new(Glyph {
                svg: SvgDecoder.decode(svg)?,
                monochrome,
            }))
        }
    }

    /// An [`IconSource`] as an asset transport, so a loader can resolve through it. It marks each icon with its tint decision, which needs the set the bytes alone do not carry.
    struct SourceTransport(Arc<dyn IconSource>);

    impl AssetTransport for SourceTransport {
        fn load(&self, key: &AssetKey, reply: Reply) {
            let answer = IconId::from_path(&key.id)
                .and_then(|id| self.0.icon(&id))
                .map_err(|error| AssetError(error.to_string()))
                .and_then(|icon| {
                    let icon = icon.ok_or_else(|| {
                        AssetError(format!("no icon `{}` in {}", key.id, self.0.describe()))
                    })?;
                    let mark = match icon.monochrome() {
                        true => MONOCHROME_MARK,
                        false => PALETTE_MARK,
                    };
                    Ok([mark, icon.svg.as_bytes()].concat())
                });
            reply(answer);
        }
    }
}
