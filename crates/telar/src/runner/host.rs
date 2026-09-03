//! Who builds a surface's renderer, and what keeps it running.
//!
//! The frame loop needs a renderer without being allowed to know what one is made of: the two Telar ships need a
//! window that hands out `raw-window-handle` handles, and an installed one may be drawing to a terminal that has
//! none. So [`AppHandler`](super::handler::AppHandler) holds a `dyn RendererHost` built by whichever entry point
//! started the app — the one place where the window type is still concrete enough to say what it can do.
//!
//! The whole renderer lifecycle lives here too: the background build, the device kept warm across a suspend and
//! the render thread are all answers a *particular* renderer gives.

mod builtin;

use std::sync::mpsc::{Receiver, SyncSender};

use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use renderer_core::{DrawCommand, RenderBackend, RendererError, RendererFactory};

use crate::config::RendererBackend;

use super::frame_thread::{FrameMsg, spawn_render_thread};

pub(super) use builtin::BuiltinHost;

/// A window the renderers Telar ships can draw on.
///
/// They ask for `raw-window-handle` handles and hold the window from the render thread, which is more than
/// [`platform_core::Window`] promises. Stating it here keeps the requirement with the renderers that have it,
/// rather than on every backend that implements a window.
pub trait SurfaceWindow:
    platform_core::Window + Clone + Send + Sync + HasWindowHandle + HasDisplayHandle + 'static
{
}

impl<W> SurfaceWindow for W where
    W: platform_core::Window + Clone + Send + Sync + HasWindowHandle + HasDisplayHandle + 'static
{
}

/// How a surface answers "what are your OS handles", for an app that embeds native content beside Telar's own
/// frames. Captured as a function pointer by the entry point that knows the window type has any.
pub(super) type RawHandles<W> = fn(&W) -> (Option<RawWindowHandle>, Option<RawDisplayHandle>);

fn raw_handles_of<W: HasWindowHandle + HasDisplayHandle>(
    window: &W,
) -> (Option<RawWindowHandle>, Option<RawDisplayHandle>) {
    (
        window.window_handle().ok().map(|h| h.as_raw()),
        window.display_handle().ok().map(|h| h.as_raw()),
    )
}

/// What depends on the window type: who builds this surface's renderer, and whether it has OS handles to give.
pub(super) struct SurfaceRenderer<W> {
    pub(super) host: Box<dyn RendererHost<W>>,
    pub(super) raw_handles: Option<RawHandles<W>>,
}

impl<W: SurfaceWindow> SurfaceRenderer<W> {
    /// The renderers Telar ships, picked by [`RendererBackend`] and the user's prefs.
    pub(super) fn builtin() -> Self {
        Self {
            host: Box::new(BuiltinHost::new()),
            raw_handles: Some(raw_handles_of::<W>),
        }
    }
}

impl<W: 'static> SurfaceRenderer<W> {
    /// A renderer installed from outside. Raw handles come back `None`: nothing here promises the window has any.
    pub(super) fn installed<F: RendererFactory<W>>(factory: F) -> Self {
        Self {
            host: Box::new(FactoryHost::new(factory)),
            raw_handles: None,
        }
    }
}

/// The frame channels of a running render thread: composed frames out, spent command buffers back.
pub(super) struct RenderChannels {
    pub(super) tx: SyncSender<FrameMsg>,
    pub(super) ret_rx: Receiver<Vec<DrawCommand>>,
}

/// What a renderer is built from that the handler happens to own: the app's fonts, paths and surface facts.
pub(super) struct RendererRequest<'a> {
    /// Which built-in to build. A host holding a factory ignores it — an installed renderer is not one of these.
    pub(super) backend: RendererBackend,
    pub(super) transparent: bool,
    pub(super) font_paths: &'a [std::path::PathBuf],
    pub(super) font_data: &'a [Vec<u8>],
    /// The family this surface's unstyled text shapes in, from its own `AppConfig`.
    pub(super) font_family: Option<&'a str>,
    pub(super) paths: &'a dyn services_core::AppPathsProvider,
    /// Names the shader-cache directory, which only the hardware build has.
    #[cfg_attr(not(feature = "hardware"), allow(dead_code))]
    pub(super) app_name: &'a str,
}

/// What came of asking a host to bring a renderer up.
pub(super) enum RendererStart {
    Started {
        /// Whether to keep submitting frames while the screen is idle. Hardware does, to hold the GPU in an active
        /// power state; re-rasterising an unchanged frame on the CPU buys nothing.
        keepalive: bool,
        label: &'static str,
    },
    /// A build is in flight; until it lands there is no renderer and frames are dropped. Only wgpu takes long
    /// enough to be worth building in the background.
    #[cfg_attr(not(feature = "hardware"), allow(dead_code))]
    Building,
    Failed(RendererError),
}

/// Whoever knows how to build and drive this surface's renderer.
pub(super) trait RendererHost<W>: 'static {
    /// Brings a renderer up for `window` and puts it on its own thread.
    fn start(&mut self, window: &W, req: &RendererRequest<'_>) -> RendererStart;

    /// Collects a build left running in the background. `None` while there is none or it is still going —
    /// [`is_building`](Self::is_building) tells those apart, because the second has to keep frames coming.
    fn poll(&mut self) -> Option<RendererStart> {
        None
    }

    fn is_building(&self) -> bool {
        false
    }

    /// Whether this host's renderer shapes text from font files. See [`RendererFactory::shapes_text`].
    fn shapes_text(&self) -> bool {
        true
    }

    /// The channels of the running renderer, or `None` when there is none to send to.
    fn channels(&self) -> Option<&RenderChannels>;

    /// Retires the render thread, keeping what makes the next [`start`](Self::start) cheap — for hardware, the
    /// device with its pipelines and warm caches.
    fn suspend(&mut self);

    /// Retires the render thread and keeps nothing, for a rebuild: transparency and the backend choice are baked
    /// into a renderer, so an app that changed its mind must not be handed the old one back.
    fn retire(&mut self);

    /// The renderer for an offscreen surface, driven inline because whoever asked for the frame reads its pixels
    /// back in the same call. `None` when this host cannot draw without a surface.
    fn build_offscreen(
        &mut self,
        _window: &W,
        _req: &RendererRequest<'_>,
    ) -> Option<Box<dyn RenderBackend>> {
        None
    }
}

/// Drives an installed [`RendererFactory`] — the host for a frontend Telar knows nothing about.
///
/// Builds inline where the built-in host goes off-thread: a factory's build is its own business, and there is no
/// `Auto` to fall back from and no device worth keeping warm.
pub(super) struct FactoryHost<W, F> {
    factory: F,
    channels: Option<RenderChannels>,
    join: Option<std::thread::JoinHandle<Box<dyn RenderBackend + Send>>>,
    _window: std::marker::PhantomData<W>,
}

impl<W, F> FactoryHost<W, F> {
    pub(super) fn new(factory: F) -> Self {
        Self {
            factory,
            channels: None,
            join: None,
            _window: std::marker::PhantomData,
        }
    }
}

impl<W, F> FactoryHost<W, F>
where
    W: 'static,
    F: RendererFactory<W>,
{
    fn build(
        &self,
        window: &W,
        req: &RendererRequest<'_>,
    ) -> Result<Box<dyn RenderBackend + Send>, RendererError> {
        let fonts = super::font_config::build_font_config(
            req.font_paths.to_vec(),
            req.font_data.to_vec(),
            req.font_family.map(str::to_owned),
            &super::font_config::SystemFonts::from_provider(req.paths),
        );
        self.factory.build(
            window,
            renderer_core::RendererBuild {
                fonts: &fonts,
                transparent: req.transparent,
            },
        )
    }
}

impl<W, F> RendererHost<W> for FactoryHost<W, F>
where
    W: 'static,
    F: RendererFactory<W>,
{
    fn start(&mut self, window: &W, req: &RendererRequest<'_>) -> RendererStart {
        let renderer = match self.build(window, req) {
            Ok(renderer) => renderer,
            Err(e) => return RendererStart::Failed(e),
        };
        let (tx, ret_rx, join) = spawn_render_thread(renderer);
        self.channels = Some(RenderChannels { tx, ret_rx });
        self.join = Some(join);
        RendererStart::Started {
            keepalive: false,
            label: "installed renderer",
        }
    }

    fn shapes_text(&self) -> bool {
        self.factory.shapes_text()
    }

    fn channels(&self) -> Option<&RenderChannels> {
        self.channels.as_ref()
    }

    fn suspend(&mut self) {
        self.retire();
    }

    fn retire(&mut self) {
        // The thread parks on the frame channel, so it only ever exits once the sender is gone.
        self.channels = None;
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }

    fn build_offscreen(
        &mut self,
        window: &W,
        req: &RendererRequest<'_>,
    ) -> Option<Box<dyn RenderBackend>> {
        match self.build(window, req) {
            Ok(renderer) => Some(renderer),
            Err(e) => {
                tracing::error!("the installed renderer could not be built offscreen: {e}");
                None
            }
        }
    }
}
