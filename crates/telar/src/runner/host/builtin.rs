//! The host for the two renderers Telar ships, and the only place that names them.
//!
//! Everything wgpu-specific about the lifecycle is here: the build kept off the UI thread because creating a device takes long enough to see, the device kept warm across a suspend so a resume rebinds a surface instead of rebuilding a pipeline cache, and `Auto` degrading to the rasteriser when there is no adapter — or when wgpu was not compiled in at all.

use renderer_core::{RenderBackend, RendererError};

use crate::config::RendererBackend;

use super::{RenderChannels, RendererHost, RendererRequest, RendererStart, SurfaceWindow};
#[cfg(any(feature = "hardware", feature = "software"))]
use crate::runner::font_config::SystemFonts;
#[cfg(any(feature = "hardware", feature = "software"))]
use crate::runner::frame_thread::spawn_render_thread;

#[cfg(not(feature = "hardware"))]
const NO_HARDWARE: &str = "telar was built without its `hardware` feature, so there is no wgpu renderer to \
                           build: enable the feature, ask for the software backend, or install a renderer of \
                           your own with `run_with_platform_and_renderer`";
#[cfg(not(feature = "software"))]
const NO_SOFTWARE: &str = "telar was built without its `software` feature, so there is no rasteriser to \
                           build: enable the feature, ask for the hardware backend, or install a renderer of \
                           your own with `run_with_platform_and_renderer`";

pub(crate) struct BuiltinHost<W: SurfaceWindow> {
    channels: Option<RenderChannels>,
    // Typed per backend rather than boxed: joining hardware must hand back a concrete `HardwareRenderer` for the next resume to rebind, which a `Box<dyn RenderBackend>` could not.
    #[cfg(feature = "hardware")]
    hw_join: Option<std::thread::JoinHandle<renderer_hardware::HardwareRenderer<W>>>,
    #[cfg(feature = "software")]
    sw_join: Option<std::thread::JoinHandle<renderer_software::SoftwareRenderer<W, W>>>,
    #[cfg(feature = "hardware")]
    pending: Option<
        std::thread::JoinHandle<Result<renderer_hardware::HardwareRenderer<W>, RendererError>>,
    >,
    #[cfg(feature = "hardware")]
    warm: Option<renderer_hardware::HardwareRenderer<W>>,
    _window: std::marker::PhantomData<W>,
}

impl<W: SurfaceWindow> BuiltinHost<W> {
    pub(crate) fn new() -> Self {
        Self {
            channels: None,
            #[cfg(feature = "hardware")]
            hw_join: None,
            #[cfg(feature = "software")]
            sw_join: None,
            #[cfg(feature = "hardware")]
            pending: None,
            #[cfg(feature = "hardware")]
            warm: None,
            _window: std::marker::PhantomData,
        }
    }

    /// Waits for the render thread, keeping the hardware device when `keep_warm`: its caches, pipelines and adapter are what make a resume cheap. Software has nothing worth carrying.
    // Every reader is behind a `cfg`, so a build with neither renderer reads it nowhere.
    #[cfg_attr(
        not(any(feature = "hardware", target_os = "linux")),
        allow(unused_variables)
    )]
    fn join_thread(&mut self, keep_warm: bool) {
        // First, and load-bearing: the thread parks on the frame channel, so it only exits once the sender is gone.
        self.channels = None;
        #[cfg(feature = "hardware")]
        if let Some(join) = self.hw_join.take() {
            match join.join() {
                Ok(hardware) if keep_warm => self.warm = Some(hardware),
                Ok(_) => {}
                Err(_) => tracing::warn!("the render thread panicked, so its renderer is lost"),
            }
        }
        #[cfg(feature = "software")]
        if let Some(join) = self.sw_join.take() {
            let _ = join.join();
        }
        // A retired renderer holds the largest allocations in the process and glibc will not return them on its own. Not on the warm path, which is keeping them on purpose.
        #[cfg(target_os = "linux")]
        if !keep_warm {
            unsafe {
                libc::malloc_trim(0);
            }
        }
    }

    #[cfg(feature = "hardware")]
    fn start_hardware(&mut self, window: &W, req: &RendererRequest<'_>) -> RendererStart {
        // Only the surface is rebound, which is fast and cannot be done off this thread.
        if let Some(mut warm) = self.warm.take() {
            match warm.rebind_surface(std::sync::Arc::new(window.clone())) {
                Ok(()) => return self.run_hardware(warm),
                Err(e) => tracing::warn!("rebinding the suspended renderer failed ({e})"),
            }
        }
        self.spawn_hardware_build(window, req);
        RendererStart::Building
    }

    // `Auto` means "the best available", which without wgpu is the rasteriser. Asking for hardware outright gets told.
    #[cfg(not(feature = "hardware"))]
    fn start_hardware(&mut self, window: &W, req: &RendererRequest<'_>) -> RendererStart {
        if matches!(req.backend, RendererBackend::Auto) {
            return self.start_software(window, req);
        }
        RendererStart::Failed(RendererError::Backend(NO_HARDWARE.to_string()))
    }

    #[cfg(feature = "hardware")]
    fn run_hardware(&mut self, renderer: renderer_hardware::HardwareRenderer<W>) -> RendererStart {
        let (tx, ret_rx, join) = spawn_render_thread(renderer);
        self.channels = Some(RenderChannels { tx, ret_rx });
        self.hw_join = Some(join);
        RendererStart::Started {
            keepalive: true,
            label: "hardware (wgpu)",
        }
    }

    /// Starts building a hardware renderer off the UI thread, leaving the handle for `poll` to pick up.
    ///
    /// One strategy for all three paths that build one: first resume, a dev backend toggle and a restart.
    #[cfg(feature = "hardware")]
    fn spawn_hardware_build(&mut self, window: &W, req: &RendererRequest<'_>) {
        // A second handle for the wake below: with no renderer yet nothing this thread asks for would draw, so the building thread is the only one that can end the wait.
        let wake = window.clone();
        let window = window.clone();
        let cache_path = crate::runner::font_config::hardware_cache_path(req.app_name, req.paths);
        let font_paths = req.font_paths.to_vec();
        let font_data = req.font_data.to_vec();
        let font_family = req.font_family.map(str::to_owned);
        let system_fonts = SystemFonts::from_provider(req.paths);
        let android = cfg!(target_os = "android");
        let transparent = req.transparent;
        self.pending = Some(std::thread::spawn(move || {
            let font_config = crate::runner::font_config::build_hardware_font_config(
                font_paths,
                font_data,
                font_family,
                &system_fonts,
            );
            let built = renderer_hardware::HardwareRenderer::new(
                window,
                cache_path.as_deref(),
                android,
                font_config,
                renderer_hardware::HardwareRendererConfig { transparent },
            );
            wake.request_redraw();
            built
        }));
    }

    #[cfg(feature = "hardware")]
    fn poll_hardware_build(&mut self) -> Option<RendererStart> {
        let handle = self.pending.take()?;
        if !handle.is_finished() {
            self.pending = Some(handle);
            return None;
        }
        let built = handle.join().unwrap_or_else(|_| {
            Err(RendererError::Backend(
                "renderer build thread panicked".to_string(),
            ))
        });
        // Retire whichever backend was driving before the new one takes the surface.
        self.join_thread(false);
        Some(match built {
            Ok(renderer) => self.run_hardware(renderer),
            Err(e) => RendererStart::Failed(e),
        })
    }

    #[cfg(not(feature = "hardware"))]
    fn poll_hardware_build(&mut self) -> Option<RendererStart> {
        None
    }

    /// Builds the rasteriser and puts it on its own thread, as hardware gets. The surface is created *here*, on the UI thread, because macOS/iOS refuse to hand out Core Graphics handles anywhere else; only the renderer moves.
    #[cfg(feature = "software")]
    fn start_software(&mut self, window: &W, req: &RendererRequest<'_>) -> RendererStart {
        let config = crate::runner::font_config::build_software_renderer_config(
            req.font_paths.to_vec(),
            req.font_data.to_vec(),
            req.font_family.map(str::to_owned),
            &SystemFonts::from_provider(req.paths),
            req.transparent,
        );
        match renderer_software::SoftwareRenderer::new(window.clone(), window.clone(), config) {
            Ok(renderer) => {
                let (tx, ret_rx, join) = spawn_render_thread(renderer);
                self.channels = Some(RenderChannels { tx, ret_rx });
                self.sw_join = Some(join);
                RendererStart::Started {
                    keepalive: false,
                    label: "software",
                }
            }
            Err(e) => RendererStart::Failed(e),
        }
    }

    #[cfg(not(feature = "software"))]
    fn start_software(&mut self, _window: &W, _req: &RendererRequest<'_>) -> RendererStart {
        RendererStart::Failed(RendererError::Backend(NO_SOFTWARE.to_string()))
    }

    #[cfg(feature = "software")]
    fn build_headless(
        &mut self,
        window: &W,
        req: &RendererRequest<'_>,
    ) -> Option<Box<dyn RenderBackend>> {
        let config = crate::runner::font_config::build_software_renderer_config(
            req.font_paths.to_vec(),
            req.font_data.to_vec(),
            req.font_family.map(str::to_owned),
            &SystemFonts::from_provider(req.paths),
            req.transparent,
        );
        Some(Box::new(
            renderer_software::SoftwareRenderer::<W, W>::new_headless(
                window.width(),
                window.height(),
                config,
            ),
        ))
    }

    #[cfg(not(feature = "software"))]
    fn build_headless(
        &mut self,
        _window: &W,
        _req: &RendererRequest<'_>,
    ) -> Option<Box<dyn RenderBackend>> {
        tracing::error!("{NO_SOFTWARE}");
        None
    }
}

impl<W: SurfaceWindow> RendererHost<W> for BuiltinHost<W> {
    fn start(&mut self, window: &W, req: &RendererRequest<'_>) -> RendererStart {
        match req.backend {
            RendererBackend::Software => self.start_software(window, req),
            RendererBackend::Hardware | RendererBackend::Auto => self.start_hardware(window, req),
        }
    }

    fn poll(&mut self) -> Option<RendererStart> {
        self.poll_hardware_build()
    }

    fn is_building(&self) -> bool {
        #[cfg(feature = "hardware")]
        return self.pending.is_some();
        #[cfg(not(feature = "hardware"))]
        false
    }

    fn channels(&self) -> Option<&RenderChannels> {
        self.channels.as_ref()
    }

    fn suspend(&mut self) {
        self.join_thread(true);
    }

    fn retire(&mut self) {
        self.join_thread(false);
    }

    /// Offscreen windows have no surface for a windowed renderer to create, so this rasterises into a CPU pixmap whatever backend was configured — the headless path then needs no GPU adapter.
    fn build_offscreen(
        &mut self,
        window: &W,
        req: &RendererRequest<'_>,
    ) -> Option<Box<dyn RenderBackend>> {
        self.build_headless(window, req)
    }
}
