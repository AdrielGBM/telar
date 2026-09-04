//! The backend the runtime drives, and the wgpu device it is waiting for.

use std::cell::RefCell;
use std::rc::Rc;

use renderer_core::{
    BuiltRenderer, Color, DrawCommand, RenderBackend, RendererBuild, RendererError, RendererFactory,
};
use renderer_hardware::{HardwareRenderer, HardwareRendererConfig};

use crate::canvas::CanvasSurface;

type Gpu = HardwareRenderer<CanvasSurface>;

/// What the device build left behind.
enum Device {
    /// The promise is still in flight. Frames drawn now are dropped, which is correct rather than unfortunate: there is nothing to draw them on yet, and the runtime asks for another as soon as one can land.
    Building,
    Ready(Box<Gpu>),
    /// The browser refused. Recorded rather than retried: a machine with no WebGPU adapter will not grow one while the page is open, and a frame loop retrying a failing device every 16ms is a hot loop.
    Failed,
}

/// Draws Telar's frames into a canvas through wgpu.
pub struct WebGpuRenderer {
    device: Rc<RefCell<Device>>,
    canvas: CanvasSurface,
}

impl WebGpuRenderer {
    /// Starts the device coming up and returns a renderer that is usable — and empty — immediately.
    pub fn new(canvas: CanvasSurface, fonts: renderer_core::FontConfig, transparent: bool) -> Self {
        let device = Rc::new(RefCell::new(Device::Building));
        let build_into = Rc::clone(&device);
        let build_on = canvas.clone();
        wasm_bindgen_futures::spawn_local(async move {
            // Asked before wgpu, so a browser that cannot draw says so in words rather than throwing a `TypeError` out of the generated glue. See `probe`.
            if let Err(reason) = crate::webgpu_available().await {
                tracing::error!("telar cannot draw here: {}", reason.message());
                report_on_page(&build_on, reason.message());
                *build_into.borrow_mut() = Device::Failed;
                return;
            }
            let font_config = renderer_text::TextShaperConfig {
                font: fonts,
                ..Default::default()
            };
            let built = HardwareRenderer::new_async(
                build_on,
                // No shader cache: the browser has no directory to keep one in, and the device reports no `PIPELINE_CACHE` feature to fill it with anyway.
                None,
                false,
                font_config,
                HardwareRendererConfig { transparent },
            )
            .await;
            *build_into.borrow_mut() = match built {
                Ok(renderer) => Device::Ready(Box::new(renderer)),
                Err(e) => {
                    tracing::error!("the browser could not open a GPU device: {e}");
                    Device::Failed
                }
            };
            // The loop stopped asking for frames while there was nothing to draw them with.
            if let Some(wake) = platform_core::loop_waker() {
                wake();
            }
        });
        Self { device, canvas }
    }
}

impl RenderBackend for WebGpuRenderer {
    fn begin_frame(
        &mut self,
        width: u32,
        height: u32,
        scale_factor: f32,
        generation: u64,
    ) -> Result<(), RendererError> {
        // The backing store follows the surface even while the device is coming up, so the first frame that lands finds a canvas already the right size rather than one it has to resize mid-frame.
        self.canvas.resize(width, height);
        match &mut *self.device.borrow_mut() {
            Device::Ready(gpu) => gpu.begin_frame(width, height, scale_factor, generation),
            Device::Building | Device::Failed => Ok(()),
        }
    }

    fn render_frame(
        &mut self,
        commands: &[DrawCommand],
        clear_color: Option<Color>,
    ) -> Result<(), RendererError> {
        match &mut *self.device.borrow_mut() {
            Device::Ready(gpu) => gpu.render_frame(commands, clear_color),
            Device::Building | Device::Failed => Ok(()),
        }
    }

    /// True whether or not the device has landed: the answer decides whether the frame pipeline pre-scales every command, and it must not change under it halfway through a run.
    fn applies_scale_factor(&self) -> bool {
        true
    }
}

/// Builds a [`WebGpuRenderer`] on a canvas chosen before the app starts.
///
/// The canvas is held rather than derived from the window the runtime passes, which is what keeps this crate from having to know anything about the browser platform: whoever wires the two together picks the element.
pub struct WebGpuRendererFactory {
    canvas: CanvasSurface,
}

impl WebGpuRendererFactory {
    pub fn new(canvas: CanvasSurface) -> Self {
        Self { canvas }
    }
}

impl<W: 'static> RendererFactory<W> for WebGpuRendererFactory {
    fn build(&self, _window: &W, build: RendererBuild<'_>) -> Result<BuiltRenderer, RendererError> {
        Ok(BuiltRenderer::Inline(Box::new(WebGpuRenderer::new(
            self.canvas.clone(),
            build.fonts.clone(),
            build.transparent,
        ))))
    }
}

/// Puts the reason a frame will never arrive where somebody looking at the page can read it.
///
/// A console message is not enough on its own: the visible result of a device that will not open is a blank area, and a blank area reads as a bug in the application rather than as a browser that cannot draw.
fn report_on_page(canvas: &CanvasSurface, message: &str) {
    let Some(host) = canvas.canvas().parent_element() else {
        return;
    };
    let Ok(notice) = host.owner_document().map_or(Err(()), Ok) else {
        return;
    };
    let Ok(element) = notice.create_element("div") else {
        return;
    };
    element.set_text_content(Some(message));
    let _ = element.set_attribute(
        "style",
        "position:absolute;inset:0;display:flex;align-items:center;justify-content:center;\
         padding:2rem;font:14px/1.6 system-ui,sans-serif;text-align:center;color:#c33",
    );
    let _ = host.append_child(&element);
}
