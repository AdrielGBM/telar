//! The renderer itself: the shared GPU handles, the per-surface state, and the caches a frame draws from.

use std::collections::HashMap;
use std::collections::VecDeque;
use std::hash::{Hash, Hasher};

use rustc_hash::FxHasher;

use geometry_core::Rect;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use renderer_core::{
    Color, DrawCommand, Raster, RenderBackend, RendererError, expand_fill_layers, hash_pod_slice,
};

use wgpu::util::DeviceExt;
use wgpu::{Device, Queue, Surface, SurfaceConfiguration};

use crate::blur::{BlurParams, BlurPipeline};
use crate::composite::CompositePipeline;
use crate::config::HardwareRendererConfig;
use crate::primitives::image::{ImageInstance, ImagePipeline};
use crate::primitives::layer::LayerPipeline;
use crate::primitives::line::{LineInstance, LinePipeline};
use crate::primitives::path::{PathFillData, PathPipeline, PathVertex};
use crate::primitives::rect::{RectInstance, RectPipeline};
use crate::primitives::text::{TextInstance, TextPipeline};
use crate::primitives::{Viewport, create_viewport_bind_group_layout, upload_instances};

mod frame;
mod pool;
pub(crate) mod shadow;
mod steps;

use pool::{PooledTexture, create_viewport_pool_slot, preferred_format};
use shadow::{ShadowCacheKey, ShadowOp};
use steps::{DrawStep, flush_batch, flush_image_batch};

impl<W: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static> HardwareRenderer<W> {
    /// Leaves this renderer's cache census where another thread can read it. Called once per frame, throttled.
    ///
    /// The GPU backend has to publish for itself: a census only the CPU backend filled would read "nothing cached" on a machine rendering entirely on the GPU — silent, and wrong in exactly the case worth looking at.
    fn publish_cache_stats(&self) {
        if !renderer_cache::registry::publish_due() {
            return;
        }
        // One set for the process however many render threads report it, so it publishes under a shared identity rather than each thread's own.
        if let Some(shared) = crate::caches::with_shared(|caches| caches.stats()) {
            renderer_cache::registry::publish_shared("gpu", shared);
        }
    }
}

/// A hardware-accelerated renderer using wgpu. The `W: Send + Sync + 'static` bound is a wgpu requirement for surface creation, not an indication that this renderer is thread-safe. The renderer must only be used on the main thread alongside the reactive runtime; it is not safe to move between threads.
pub struct HardwareRenderer<W: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static> {
    instance: wgpu::Instance,
    // `None` headless: with no swapchain, frames render into `offscreen_output` instead.
    surface: Option<Surface<'static>>,
    device: Device,
    queue: Queue,
    config: Option<SurfaceConfiguration>,
    viewport_buffer: wgpu::Buffer,
    viewport_bind_group: wgpu::BindGroup,
    viewport_dirty: bool,
    // Round-robin pool of (buffer, bind group) pairs for per-layer viewport uniforms, avoiding a driver round trip per layer each frame. Reset to index 0 at `begin_frame`.
    viewport_buffer_pool: Vec<(wgpu::Buffer, wgpu::BindGroup)>,
    viewport_buffer_pool_index: usize,
    // Keyed by (width, height, format) and reused across frames, so backdrop-blur scratch targets cost no per-frame multi-megabyte allocations.
    texture_pool: Vec<(
        u32,
        u32,
        wgpu::TextureFormat,
        wgpu::Texture,
        wgpu::TextureView,
    )>,
    max_texture_pool_per_size: usize,
    rect_pipeline: RectPipeline,
    text_pipeline: TextPipeline,
    line_pipeline: LinePipeline,
    image_pipeline: ImagePipeline,
    // Queried once at construction, so dirty-rect computation does not under-estimate the text region.
    font_metrics: renderer_core::FontMetrics,
    surface_format: wgpu::TextureFormat,
    present_mode: wgpu::PresentMode,
    alpha_mode: wgpu::CompositeAlphaMode,
    width: u32,
    height: u32,
    scale_factor: f32,
    pending_instances: Vec<RectInstance>,
    pending_text_instances: Vec<TextInstance>,
    pending_line_instances: Vec<LineInstance>,
    pending_image_instances: Vec<ImageInstance>,
    pending_steps: Vec<DrawStep>,
    merge_out: Vec<DrawStep>,
    merge_zone: Vec<DrawStep>,
    glyph_scratch: Vec<renderer_text::GlyphInfo>,
    path_pipeline: PathPipeline,
    layer_pipeline: LayerPipeline,
    viewport_bind_group_layout: wgpu::BindGroupLayout,
    blur_pipeline: BlurPipeline,
    composite_pipeline: CompositePipeline,
    pending_shadow_instances: Vec<TextInstance>,
    pending_shadows: Vec<ShadowOp>,
    pending_shadow_path_vertices: Vec<PathVertex>,
    pending_shadow_path_indices: Vec<u32>,
    pending_shadow_path_fill_data: Vec<PathFillData>,
    pending_path_vertices: Vec<PathVertex>,
    pending_path_indices: Vec<u32>,
    pending_path_fill_data: Vec<PathFillData>,
    msaa_samples: u32,
    msaa_texture: Option<wgpu::Texture>,
    batch_rect_start: Option<u32>,
    batch_text_start: Option<u32>,
    batch_line_start: Option<u32>,
    batch_image_key: Option<(u64, Raster)>,
    batch_image_start: Option<u32>,
    batch_image_bind_group: Option<wgpu::BindGroup>,
    draw_state: renderer_core::DrawState,
    layer_texture_pool: Vec<PooledTexture>,
    shadow_capture_pool: Vec<PooledTexture>,
    // Keyed by a hash of all pending shadow instances, so unchanged shadows skip the per-frame create-and-bind.
    shadow_instances_cache: Option<(u64, wgpu::Buffer, wgpu::BindGroup)>,
    // Keyed by a hash of their draw commands and layer params, so unchanged static layers skip their whole render pass and composite directly.
    layer_resolved_cache: HashMap<u64, (wgpu::Texture, wgpu::TextureView, u64)>,
    // Front is least-recently-used, back is most-recently-used.
    layer_resolved_cache_order: VecDeque<u64>,
    // Set per frame to 4 * width * height.
    layer_cache_pixel_budget: u64,
    // Both the idle-frame fast-path source and the MSAA resolve target each active frame.
    retained_texture: Option<wgpu::Texture>,
    retained_view: Option<wgpu::TextureView>,
    // `Some` if and only if `surface` is `None`. The final frame lands here so `read_rgba` can copy it back; recreated on resize, unless it belongs to the application.
    offscreen_output: Option<wgpu::Texture>,
    // An app-owned target must not be replaced by `reconfigure`, and the frame is blended into it rather than copied over it, so Telar composes into whatever the application already drew.
    app_owned_target: bool,
    prev_commands: Vec<DrawCommand>,
    // Initialised to `u64::MAX`, so the first frame never matches and always renders.
    prev_generation: u64,
    // Used by `render_frame` to decide the idle-blit fast path.
    incoming_generation: u64,
    retained_blit_pipeline: crate::composite::CompositePipeline,
    prev_rect_hash: u64,
    prev_text_hash: u64,
    prev_line_hash: u64,
    prev_image_hash: u64,
    // `None` headless. Kept alive otherwise, so wgpu's surface stays valid for its lifetime.
    _window: Option<std::sync::Arc<W>>,
}

// RENDER_ATTACHMENT for the final draw, COPY_SRC for `read_rgba`, COPY_DST for the single-sample copy path.
fn create_offscreen_texture(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("telar-offscreen-output"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

// Safety: the cross-thread transfer happens before any DrawCommands are processed, so no `Rc` values exist at transfer time; after joining, the renderer lives exclusively on the main thread.
unsafe impl<W: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static> Send
    for HardwareRenderer<W>
{
}

impl<W: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static> Drop for HardwareRenderer<W> {
    fn drop(&mut self) {
        // The caller tearing a window down holds a lifecycle guard across the whole renderer and window drop, serialising it against sibling render threads. Before the device, so the driver can free their GPU memory.
        self.layer_resolved_cache.clear();
        // Destroy this window's surface before polling: with a process-shared device wgpu defers surface teardown to the next maintenance, and this window's render thread is already gone, so the VkSurfaceKHR — which keeps the wl_surface alive — would linger on screen until some other window's teardown polls. On a lost device `poll` is a fatal wgpu panic, which while unwinding from an earlier render panic would abort, so it is caught and teardown finishes cleanly.
        self.surface = None;
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        }));
    }
}

// One GPU instance, adapter, device and queue shared by every window; per-window renderers own only a `Surface`. Sharing is required for multi-window: a separate device per window, destroyed when its window closes, corrupts the shared driver and segfaults a sibling's in-flight `vkAcquireNextImageKHR`. With a shared device, closing a window drops only its swapchain.
#[derive(Clone)]
/// The one instance, adapter, device and queue every window in the process draws through.
pub struct SharedGpu {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: Device,
    pub queue: Queue,
}

pub(crate) static SHARED_GPU: std::sync::OnceLock<SharedGpu> = std::sync::OnceLock::new();

// Keeps a swapchain from being rebuilt while any window is advancing one: the WSI beneath a shared device does not take being driven from several threads at once, and reconfiguring one window's swapchain while a sibling sits inside its acquire loses the device — which is every window at once. A read/write lock rather than a mutex, because acquire and present are meant to run concurrently and only a rebuild excludes them; a mutex serialised every window's presentation behind a dragged window's reconfigure. Taken around each call rather than held across a frame: the acquire path reconfigures on `Lost`, and a read guard still held there would deadlock against the write it needs.
static SWAPCHAIN: std::sync::RwLock<()> = std::sync::RwLock::new(());

/// Held while a swapchain is advanced — acquired or presented. Shared: any number of windows at once.
pub fn swapchain_lock() -> std::sync::RwLockReadGuard<'static, ()> {
    SWAPCHAIN
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Held while a swapchain is built or rebuilt. Exclusive against every window's acquire and present.
pub(crate) fn swapchain_rebuild_lock() -> std::sync::RwLockWriteGuard<'static, ()> {
    // wgpu reports a fatal error as a panic, so the thread holding either guard can die with it, and poisoning would take down the windows that survived.
    SWAPCHAIN
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A pipeline built on a scoped thread, or the reason it could not be.
///
/// A panic here is rarely a bug in the pipeline: wgpu reports a fatal device error as a panic, so a device lost by any window arrives as one on every thread that touches it afterwards. Unwrapping would carry that into the caller — the UI thread — and end the process, which is how one dead device cost a whole application rather than the one surface that failed to open.
#[cfg(not(target_arch = "wasm32"))]
fn built<T>(handle: std::thread::ScopedJoinHandle<'_, T>) -> Result<T, RendererError> {
    handle
        .join()
        .map_err(|_| RendererError::Backend("a render pipeline could not be built".to_string()))
}

/// Opens the process-wide GPU objects, or hands back the ones a renderer already opened. Blocking, and serialized against every window's surface lifecycle for the same reason [`HardwareRenderer::new`] is.
pub(crate) fn open_shared_gpu() -> Result<SharedGpu, RendererError> {
    if let Some(gpu) = SHARED_GPU.get() {
        return Ok(gpu.clone());
    }
    let _gpu = renderer_core::gpu_sync::lifecycle_guard();
    pollster::block_on(shared_gpu(wgpu::Backends::all())).cloned()
}

/// What the instance is allowed to switch on for itself.
///
/// A debug build normally asks for the validation layers and `VK_EXT_debug_utils`, which is the right default everywhere it works. It does not work here: several Android Vulkan loaders *advertise* `VK_EXT_debug_utils` and then hand back no entry point for it, and the loader that asked panics inside a function that cannot unwind — so the process aborts before the first frame, and a debug build of any Telar app simply would not start on the phone.
///
/// Left alone on every other target: losing the validation layers is a real cost, and it is only paid where the alternative is not running at all.
fn instance_flags() -> wgpu::InstanceFlags {
    let flags = wgpu::InstanceFlags::from_build_config();
    if cfg!(target_os = "android") {
        flags.difference(wgpu::InstanceFlags::DEBUG | wgpu::InstanceFlags::VALIDATION)
    } else {
        flags
    }
}

async fn shared_gpu(backends: wgpu::Backends) -> Result<&'static SharedGpu, RendererError> {
    if let Some(gpu) = SHARED_GPU.get() {
        return Ok(gpu);
    }
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends,
        flags: instance_flags(),
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    // No `compatible_surface`: the shared adapter serves every window, and on a normal desktop any window's surface is presentable by whichever this picks. Which one belongs to the application, because only it knows whether its frame is a menu or a viewport.
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: crate::gpu::preference(),
            compatible_surface: None,
            ..Default::default()
        })
        .await
        .map_err(|_| RendererError::Backend("no suitable GPU adapter found".to_string()))?;

    const BLUR_PARAMS_SIZE: u32 = std::mem::size_of::<BlurParams>() as u32;
    let pipeline_cache_feature = if adapter.features().contains(wgpu::Features::PIPELINE_CACHE) {
        wgpu::Features::PIPELINE_CACHE
    } else {
        wgpu::Features::empty()
    };
    let supports_immediates = adapter.features().contains(wgpu::Features::IMMEDIATES)
        && adapter.limits().max_immediate_size >= BLUR_PARAMS_SIZE;
    let immediates_feature = if supports_immediates {
        wgpu::Features::IMMEDIATES
    } else {
        wgpu::Features::empty()
    };
    let mut required_limits = wgpu::Limits::default();
    if supports_immediates {
        required_limits.max_immediate_size = BLUR_PARAMS_SIZE;
    }
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("telar-hardware-renderer"),
            required_features: pipeline_cache_feature | immediates_feature,
            required_limits,
            ..Default::default()
        })
        .await
        .map_err(|e| RendererError::Backend(format!("GPU device request failed: {}", e)))?;

    // A concurrent initializer may have won the race; keep whichever landed first. Dropping the loser's handles is safe, since no surface is bound to them.
    let _ = SHARED_GPU.set(SharedGpu {
        instance,
        adapter,
        device,
        queue,
    });
    Ok(SHARED_GPU.get().expect("shared GPU just set"))
}

// Seeds the offscreen with the previous frame shifted by the scroll delta, so a cleared scrolling frame redraws only the exposed band. `TELAR_HW_SCROLL_BLIT=0` falls back to a full re-render.
fn hw_scroll_blit_enabled() -> bool {
    use std::sync::OnceLock;
    static V: OnceLock<bool> = OnceLock::new();
    *V.get_or_init(|| std::env::var("TELAR_HW_SCROLL_BLIT").as_deref() != Ok("0"))
}

// Generalises the scroll-blit prime to an arbitrary dirty rect, so a frame that changed only a small region repaints just the dirty scissor. `TELAR_HW_DAMAGE=0` forces a full re-render.
fn hw_damage_with_clear_enabled() -> bool {
    use std::sync::OnceLock;
    static V: OnceLock<bool> = OnceLock::new();
    *V.get_or_init(|| std::env::var("TELAR_HW_DAMAGE").as_deref() != Ok("0"))
}

fn cull_bounds(
    bounds: geometry_core::Rect,
    scissor: Option<geometry_core::Rect>,
    dirty_scissor: Option<geometry_core::Rect>,
    scroll_blit: Option<&renderer_core::ScrollBlit>,
) -> bool {
    if !renderer_core::culling::overlaps(bounds.x, bounds.y, bounds.width, bounds.height, scissor) {
        return true;
    }
    if let Some(ds) = dirty_scissor {
        if !renderer_core::culling::overlaps(
            bounds.x,
            bounds.y,
            bounds.width,
            bounds.height,
            Some(ds),
        ) {
            return true;
        }
    }
    if let Some(sb) = scroll_blit {
        let in_exp = renderer_core::culling::overlaps(
            bounds.x,
            bounds.y,
            bounds.width,
            bounds.height,
            Some(sb.exposed_band),
        );
        let in_extra = sb.extra_dirty.iter().any(|ed| {
            renderer_core::culling::overlaps(
                bounds.x,
                bounds.y,
                bounds.width,
                bounds.height,
                Some(*ed),
            )
        });
        if !in_exp && !in_extra {
            return true;
        }
    }
    false
}

// Width and height are at least 1, since wgpu rejects empty scissors.
fn physical_scissor(rect: Rect, width: u32, height: u32, scale: f32) -> (u32, u32, u32, u32) {
    let x = ((rect.x * scale).max(0.0).floor() as u32).min(width.saturating_sub(1));
    let y = ((rect.y * scale).max(0.0).floor() as u32).min(height.saturating_sub(1));
    let right = (((rect.x + rect.width) * scale).ceil() as u32).min(width);
    let bottom = (((rect.y + rect.height) * scale).ceil() as u32).min(height);
    let w = right.saturating_sub(x).max(1).min(width.saturating_sub(x));
    let h = bottom
        .saturating_sub(y)
        .max(1)
        .min(height.saturating_sub(y));
    (x, y, w, h)
}

impl<W: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static> HardwareRenderer<W> {
    pub fn new(
        window: W,
        cache_path: Option<&std::path::Path>,
        vulkan_only: bool,
        font_config: renderer_text::TextShaperConfig,
        config: HardwareRendererConfig,
    ) -> Result<Self, RendererError> {
        // Exclusive against every render thread: creating a device or surface must not overlap another window's in-flight acquire or present.
        let _gpu = renderer_core::gpu_sync::lifecycle_guard();
        pollster::block_on(Self::new_async(
            window,
            cache_path,
            vulkan_only,
            font_config,
            config,
        ))
    }

    pub async fn new_async(
        window: W,
        cache_path: Option<&std::path::Path>,
        vulkan_only: bool,
        font_config: renderer_text::TextShaperConfig,
        config: HardwareRendererConfig,
    ) -> Result<Self, RendererError> {
        let window = std::sync::Arc::new(window);

        let backends = if vulkan_only {
            wgpu::Backends::VULKAN
        } else {
            wgpu::Backends::all()
        };
        // The process-wide instance, adapter and device are shared; this window owns only its surface.
        let gpu = shared_gpu(backends).await?;
        let instance = gpu.instance.clone();
        let adapter = gpu.adapter.clone();

        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| RendererError::Surface(e.to_string()))?;

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = preferred_format(&surface_caps);
        // Adreno TBDR GPUs silently drop MSAA samples across render-pass boundaries, yielding zeros.
        let msaa_samples = if cfg!(target_os = "android") {
            1
        } else if adapter
            .get_texture_format_features(surface_format)
            .flags
            .contains(wgpu::TextureFormatFeatureFlags::MULTISAMPLE_X4)
        {
            4
        } else {
            1
        };
        // Mailbox on some Adreno and MIUI devices silently drops frames, producing a black screen.
        let present_mode = if cfg!(target_os = "android") {
            wgpu::PresentMode::Fifo
        } else {
            surface_caps
                .present_modes
                .iter()
                .find(|&&m| m == wgpu::PresentMode::Mailbox)
                .copied()
                .unwrap_or(wgpu::PresentMode::Fifo)
        };
        // A transparent app needs the compositor to blend its surface; a normal app prefers Opaque. Pick the first mode the surface actually offers, falling back to whatever it has.
        let preferred: &[wgpu::CompositeAlphaMode] = if config.transparent {
            &[
                wgpu::CompositeAlphaMode::PreMultiplied,
                wgpu::CompositeAlphaMode::PostMultiplied,
                wgpu::CompositeAlphaMode::Inherit,
            ]
        } else {
            &[wgpu::CompositeAlphaMode::Opaque]
        };
        let alpha_mode = preferred
            .iter()
            .find(|m| surface_caps.alpha_modes.contains(m))
            .copied()
            .unwrap_or_else(|| {
                surface_caps
                    .alpha_modes
                    .first()
                    .copied()
                    .unwrap_or(wgpu::CompositeAlphaMode::Auto)
            });
        tracing::info!(
            "hw init: format={:?} msaa={} alpha={:?} present={:?} all_formats={:?} all_alpha={:?}",
            surface_format,
            msaa_samples,
            alpha_mode,
            present_mode,
            surface_caps.formats,
            surface_caps.alpha_modes,
        );

        Self::from_parts(
            instance,
            adapter,
            gpu.device.clone(),
            gpu.queue.clone(),
            Some(surface),
            Some(window),
            surface_format,
            msaa_samples,
            present_mode,
            alpha_mode,
            cache_path,
            font_config,
        )
        .await
    }

    // Format, msaa, present and alpha are decided by the caller, since only the windowed path can query surface capabilities.
    async fn from_parts(
        instance: wgpu::Instance,
        adapter: wgpu::Adapter,
        device: Device,
        queue: Queue,
        surface: Option<Surface<'static>>,
        window: Option<std::sync::Arc<W>>,
        surface_format: wgpu::TextureFormat,
        msaa_samples: u32,
        present_mode: wgpu::PresentMode,
        alpha_mode: wgpu::CompositeAlphaMode,
        cache_path: Option<&std::path::Path>,
        font_config: renderer_text::TextShaperConfig,
    ) -> Result<Self, RendererError> {
        // Off the device, not the adapter: an adopted device may lack a feature its adapter advertises, and taking the immediates path against one is a validation error rather than a slow path.
        const BLUR_PARAMS_SIZE: u32 = std::mem::size_of::<BlurParams>() as u32;
        let supports_immediates = device.features().contains(wgpu::Features::IMMEDIATES)
            && device.limits().max_immediate_size >= BLUR_PARAMS_SIZE;

        // `None` on non-Vulkan backends, where pipeline caching is unsupported.
        let (pipeline_cache, cache_file_path) = {
            let adapter_info = adapter.get_info();
            let key = wgpu::util::pipeline_cache_key(&adapter_info)
                .filter(|_| device.features().contains(wgpu::Features::PIPELINE_CACHE));
            if let (Some(key), Some(base)) = (key, cache_path) {
                let path = base.join(key);
                let data = std::fs::read(&path).ok();
                let cache = unsafe {
                    device.create_pipeline_cache(&wgpu::PipelineCacheDescriptor {
                        label: Some("telar-pipeline-cache"),
                        data: data.as_deref(),
                        fallback: true,
                    })
                };
                (Some(cache), Some(path))
            } else {
                (None, None)
            }
        };

        let viewport_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("telar-viewport"),
            size: std::mem::size_of::<Viewport>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let viewport_bind_group_layout = create_viewport_bind_group_layout(&device);
        let viewport_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("telar-viewport-bg"),
            layout: &viewport_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: viewport_buffer.as_entire_binding(),
            }],
        });

        // Builds this thread's shared caches on first use and yields handles on the one glyph atlas. Taken before the scope below, because the text pipeline is built on a spawned thread a thread-local borrow cannot cross.
        let (atlas_bgl, atlas_bind_group) =
            crate::caches::atlas_handles(&device, font_config.font.clone());

        // Defined out here so its lifetime covers the spawned threads. `ImagePipeline` is absent: its `Rc` cache must be built on this thread.
        let pc = pipeline_cache.as_ref();
        // Each pipeline's construction as a closure, so running them in parallel or one after another shares one set of definitions.
        let make_rect = || {
            RectPipeline::new(
                &device,
                surface_format,
                &viewport_bind_group_layout,
                pc,
                msaa_samples,
            )
        };
        let make_text = || {
            TextPipeline::new(
                &device,
                surface_format,
                &viewport_bind_group_layout,
                pc,
                msaa_samples,
                &atlas_bgl,
                atlas_bind_group.clone(),
            )
        };
        let make_line = || {
            LinePipeline::new(
                &device,
                surface_format,
                &viewport_bind_group_layout,
                pc,
                msaa_samples,
            )
        };
        let make_path = || {
            PathPipeline::new(
                &device,
                surface_format,
                &viewport_bind_group_layout,
                pc,
                msaa_samples,
            )
        };
        let make_layer = || LayerPipeline::new(&device, surface_format, msaa_samples);
        let make_blur = || BlurPipeline::new(&device, surface_format, pc, supports_immediates);
        let make_composite = || {
            CompositePipeline::new(
                &device,
                surface_format,
                msaa_samples,
                &viewport_bind_group_layout,
                pc,
            )
        };
        let make_retained =
            || CompositePipeline::new(&device, surface_format, 1, &viewport_bind_group_layout, pc);

        // On Vulkan and Metal this takes startup from about eight serial shader compilations to one critical path.
        #[cfg(not(target_arch = "wasm32"))]
        let (
            rect_pipeline,
            text_pipeline,
            line_pipeline,
            path_pipeline,
            layer_pipeline,
            blur_pipeline,
            composite_pipeline,
            retained_blit_pipeline,
        ) = std::thread::scope(|s| {
            let t_rect = s.spawn(make_rect);
            let t_text = s.spawn(make_text);
            let t_line = s.spawn(make_line);
            let t_path = s.spawn(make_path);
            let t_layer = s.spawn(make_layer);
            let t_blur = s.spawn(make_blur);
            let t_composite = s.spawn(make_composite);
            let t_retained = s.spawn(make_retained);
            Ok::<_, RendererError>((
                built(t_rect)?,
                built(t_text)?,
                built(t_line)?,
                built(t_path)?,
                built(t_layer)?,
                built(t_blur)?,
                built(t_composite)?,
                built(t_retained)?,
            ))
        })?;
        // The browser has one thread, where `std::thread::scope` is not slower but absent: it panics.
        #[cfg(target_arch = "wasm32")]
        let (
            rect_pipeline,
            text_pipeline,
            line_pipeline,
            path_pipeline,
            layer_pipeline,
            blur_pipeline,
            composite_pipeline,
            retained_blit_pipeline,
        ) = (
            make_rect(),
            make_text(),
            make_line(),
            make_path(),
            make_layer(),
            make_blur(),
            make_composite(),
            make_retained(),
        );
        // After the parallel scope, because it needs the shared image layout, which the borrow of the shared set cannot cross a spawned thread to reach.
        let image_bgl =
            crate::caches::with_shared(|caches| caches.images.bind_group_layout.clone())
                .expect("atlas_handles above built the shared caches");
        let image_pipeline = ImagePipeline::new(
            &device,
            surface_format,
            &viewport_bind_group_layout,
            pipeline_cache.as_ref(),
            msaa_samples,
            &image_bgl,
        );

        // So subsequent startups skip shader compilation.
        if let (Some(cache), Some(path)) = (pipeline_cache, cache_file_path) {
            if let Some(data) = cache.get_data() {
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let tmp = path.with_extension("tmp");
                if std::fs::write(&tmp, &data).is_ok() {
                    let _ = std::fs::rename(&tmp, &path);
                }
            }
        }

        // So the common case of few layers per frame never allocates during rendering.
        let mut viewport_buffer_pool = Vec::with_capacity(crate::limits::VIEWPORT_POOL_SIZE);
        for _ in 0..crate::limits::VIEWPORT_POOL_SIZE {
            viewport_buffer_pool.push(create_viewport_pool_slot(
                &device,
                &viewport_bind_group_layout,
            ));
        }

        // From the shared shaper, which `atlas_handles` already built with this font config.
        let font_metrics = crate::caches::with_shared(|caches| caches.text_shaper.font_metrics())
            .expect("atlas_handles above built the shared caches");

        Ok(Self {
            instance,
            surface,
            device,
            queue,
            config: None,
            viewport_buffer,
            viewport_bind_group,
            viewport_dirty: true,
            viewport_buffer_pool,
            viewport_buffer_pool_index: 0,
            texture_pool: Vec::new(),
            max_texture_pool_per_size: crate::limits::MAX_TEXTURE_POOL_PER_SIZE,
            rect_pipeline,
            text_pipeline,
            line_pipeline,
            image_pipeline,
            path_pipeline,
            layer_pipeline,
            viewport_bind_group_layout,
            blur_pipeline,
            composite_pipeline,
            pending_shadow_instances: Vec::new(),
            pending_shadows: Vec::new(),
            pending_shadow_path_vertices: Vec::new(),
            pending_shadow_path_indices: Vec::new(),
            pending_shadow_path_fill_data: Vec::new(),
            font_metrics,
            surface_format,
            present_mode,
            alpha_mode,
            width: 0,
            height: 0,
            scale_factor: 1.0,
            pending_instances: Vec::new(),
            pending_text_instances: Vec::new(),
            pending_line_instances: Vec::new(),
            pending_image_instances: Vec::new(),
            pending_steps: Vec::new(),
            merge_out: Vec::new(),
            merge_zone: Vec::new(),
            glyph_scratch: Vec::new(),
            pending_path_vertices: Vec::new(),
            pending_path_indices: Vec::new(),
            pending_path_fill_data: Vec::new(),
            msaa_samples,
            msaa_texture: None,
            batch_rect_start: None,
            batch_text_start: None,
            batch_line_start: None,
            batch_image_key: None,
            batch_image_start: None,
            batch_image_bind_group: None,
            draw_state: renderer_core::DrawState::new(),
            layer_texture_pool: Vec::new(),
            shadow_capture_pool: Vec::new(),
            shadow_instances_cache: None,
            layer_resolved_cache: HashMap::new(),
            layer_resolved_cache_order: VecDeque::new(),
            layer_cache_pixel_budget: 0,
            retained_texture: None,
            retained_view: None,
            offscreen_output: None,
            app_owned_target: false,
            prev_commands: Vec::new(),
            prev_generation: u64::MAX,
            incoming_generation: 0,
            retained_blit_pipeline,
            prev_rect_hash: 0,
            prev_text_hash: 0,
            prev_line_hash: 0,
            prev_image_hash: 0,
            _window: window,
        })
    }

    /// Rebind the renderer to a new native window after Android resume. Keeps all GPU resources (device, pipelines, caches, atlas) intact — only the surface is replaced. The new surface will be configured on the next `begin_frame` call.
    pub fn rebind_surface(&mut self, window: std::sync::Arc<W>) -> Result<(), RendererError> {
        let new_surface = self
            .instance
            .create_surface(window.clone())
            .map_err(|e| RendererError::Surface(e.to_string()))?;
        self.surface = Some(new_surface);
        self._window = Some(window);
        // `begin_frame` handles `config.is_none()`.
        self.config = None;
        self.viewport_dirty = true;
        Ok(())
    }

    /// Reads the headless offscreen target back as tightly-packed RGBA8 bytes (row-major, `width * height * 4`, no row padding). Channel order matches the renderer's `surface_format` (Rgba8Unorm on the headless path, i.e. R,G,B,A). Errors in windowed mode (no offscreen target) or if buffer mapping fails. Blocks on the GPU copy, so call it after `render_frame`.
    pub fn read_rgba(&self) -> Result<Vec<u8>, RendererError> {
        let texture = self.offscreen_output.as_ref().ok_or_else(|| {
            RendererError::Backend("read_rgba requires headless mode (no offscreen target)".into())
        })?;
        let width = texture.width();
        let height = texture.height();
        let unpadded_bytes_per_row = width * 4;
        // Buffer copies require each row to start at a 256-byte boundary, so the stride is padded and stripped after mapping.
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(align) * align;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("telar-readback"),
            size: (padded_bytes_per_row * height) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("telar-readback-encoder"),
            });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(std::iter::once(encoder.finish()));

        let slice = buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| RendererError::Backend(format!("readback poll failed: {e:?}")))?;

        let data = slice
            .get_mapped_range()
            .map_err(|e| RendererError::Backend(format!("readback map failed: {e:?}")))?;
        let mut out = Vec::with_capacity((unpadded_bytes_per_row * height) as usize);
        for row in 0..height {
            let start = (row * padded_bytes_per_row) as usize;
            out.extend_from_slice(&data[start..start + unpadded_bytes_per_row as usize]);
        }
        drop(data);
        buffer.unmap();
        Ok(out)
    }

    /// Blocks until all submitted GPU work has completed. Intended for headless/benchmark/test use where there is no present() to pace the queue; the windowed runtime never needs this.
    pub fn wait_idle(&self) -> Result<(), RendererError> {
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map(|_| ())
            .map_err(|e| RendererError::Backend(format!("poll failed: {e:?}")))
    }

    // Reuses a pooled slot round-robin, writing the new contents in place, and grows the pool otherwise. The returned bind group is an Arc-backed clone, so the pool keeps ownership.
    fn take_layer_viewport_bind_group(&mut self, viewport: Viewport) -> wgpu::BindGroup {
        let idx = self.viewport_buffer_pool_index;
        self.viewport_buffer_pool_index += 1;
        if idx >= self.viewport_buffer_pool.len() {
            let slot = create_viewport_pool_slot(&self.device, &self.viewport_bind_group_layout);
            self.queue
                .write_buffer(&slot.0, 0, bytemuck::bytes_of(&viewport));
            let bg = slot.1.clone();
            self.viewport_buffer_pool.push(slot);
            return bg;
        }
        let (buffer, bind_group) = &self.viewport_buffer_pool[idx];
        self.queue
            .write_buffer(buffer, 0, bytemuck::bytes_of(&viewport));
        bind_group.clone()
    }

    // A zero rect and radius restores the unclipped viewport.
    fn take_shader_clip_viewport_bind_group(
        &mut self,
        clip_rect: Rect,
        clip_radius: f32,
    ) -> wgpu::BindGroup {
        let mut viewport = Viewport::new(
            [self.width as f32, self.height as f32],
            [0.0; 2],
            self.scale_factor,
        );
        viewport.clip_rect = [clip_rect.x, clip_rect.y, clip_rect.width, clip_rect.height];
        viewport.clip_radius = clip_radius;
        self.take_layer_viewport_bind_group(viewport)
    }

    fn reconfigure(&mut self, width: u32, height: u32) {
        // COPY_DST is needed for the single-sample copy path.
        let surface_usage = if self.msaa_samples == 1 {
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_DST
        } else {
            wgpu::TextureUsages::RENDER_ATTACHMENT
        };
        let config = SurfaceConfiguration {
            usage: surface_usage,
            format: self.surface_format,
            // `Auto` is wgpu's pre-30 behaviour: sRGB for our 8-bit formats, chosen by the backend.
            color_space: wgpu::SurfaceColorSpace::Auto,
            width,
            height,
            present_mode: self.present_mode,
            alpha_mode: self.alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        // Headless has no surface to configure, but the config is still kept so the same `config.is_some()` gating applies unchanged.
        if let Some(surface) = self.surface.as_ref() {
            let _swapchain = swapchain_rebuild_lock();
            surface.configure(&self.device, &config);
        }
        self.config = Some(config);
        self.viewport_dirty = true;
        // At one sample the resolve is a texture copy and the idle-blit samples this texture directly. A multisample texture cannot be sampled, so TEXTURE_BINDING is added only on the single-sample branch.
        let msaa_usage = if self.msaa_samples == 1 {
            wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING
        } else {
            wgpu::TextureUsages::RENDER_ATTACHMENT
        };
        self.msaa_texture = Some(self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("telar-msaa"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: self.msaa_samples,
            dimension: wgpu::TextureDimension::D2,
            format: self.surface_format,
            usage: msaa_usage,
            view_formats: &[],
        }));
        // At one sample the retained texture is the copy destination, so COPY_DST is required.
        let retained_usage = if self.msaa_samples == 1 {
            wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
        } else {
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING
        };
        let retained = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("telar-retained"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.surface_format,
            usage: retained_usage,
            view_formats: &[],
        });
        self.retained_view = Some(retained.create_view(&wgpu::TextureViewDescriptor::default()));
        self.retained_texture = Some(retained);
        // The windowed path presents to the swapchain and has no offscreen target; an app-owned one is sized by whoever owns it, and replacing it here would drop the picture on the floor.
        if self.surface.is_none() && !self.app_owned_target {
            self.offscreen_output = Some(create_offscreen_texture(
                &self.device,
                self.surface_format,
                width,
                height,
            ));
        }
        // So a scroll blit is never applied across a size change.
        self.prev_commands.clear();
    }

    fn clear_pending(&mut self) {
        self.pending_instances.clear();
        self.pending_text_instances.clear();
        self.pending_line_instances.clear();
        self.pending_image_instances.clear();
        self.pending_steps.clear();
        self.pending_path_vertices.clear();
        self.pending_path_indices.clear();
        self.pending_path_fill_data.clear();
        self.batch_rect_start = None;
        self.batch_text_start = None;
        self.batch_line_start = None;
        self.batch_image_key = None;
        self.batch_image_start = None;
        self.batch_image_bind_group = None;
        self.pending_shadow_instances.clear();
        self.pending_shadows.clear();
        self.pending_shadow_path_vertices.clear();
        self.pending_shadow_path_indices.clear();
        self.pending_shadow_path_fill_data.clear();
    }

    fn flush_rect(&mut self) {
        if self.batch_rect_start.is_none() {
            return;
        }
        flush_batch(
            &mut self.pending_steps,
            &mut self.batch_rect_start,
            self.pending_instances.len() as u32,
            |start, end| DrawStep::RectBatch { start, end },
        );
    }

    fn flush_text(&mut self) {
        if self.batch_text_start.is_none() {
            return;
        }
        flush_batch(
            &mut self.pending_steps,
            &mut self.batch_text_start,
            self.pending_text_instances.len() as u32,
            |start, end| DrawStep::TextBatch { start, end },
        );
    }

    fn flush_line(&mut self) {
        if self.batch_line_start.is_none() {
            return;
        }
        flush_batch(
            &mut self.pending_steps,
            &mut self.batch_line_start,
            self.pending_line_instances.len() as u32,
            |start, end| DrawStep::LineBatch { start, end },
        );
    }

    fn flush_image(&mut self) {
        if self.batch_image_start.is_none() {
            return;
        }
        flush_image_batch(
            &mut self.pending_steps,
            &mut self.batch_image_start,
            &mut self.batch_image_bind_group,
            &mut self.batch_image_key,
            self.pending_image_instances.len() as u32,
        );
    }

    fn flush_all(&mut self) {
        self.flush_rect();
        self.flush_text();
        self.flush_line();
        self.flush_image();
    }
}

// Headless needs no window, so `W` is a pure phantom — kept generic so the caller picks any window type without this crate depending on it.
impl<W: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static> HardwareRenderer<W> {
    /// Build a windowless renderer that draws into an offscreen texture instead of a swapchain surface. Read the rendered frame back with [`HardwareRenderer::read_rgba`]. Same font/cache/config parameters as [`HardwareRenderer::new_async`] minus the window; `width`/`height` are the initial physical target size and are re-derived from `begin_frame` on the first frame.
    pub async fn new_headless(
        width: u32,
        height: u32,
        cache_path: Option<&std::path::Path>,
        vulkan_only: bool,
        font_config: renderer_text::TextShaperConfig,
    ) -> Result<Self, RendererError> {
        // Rgba8Unorm is a mandatory renderable format and the windowed path's first choice, so `read_rgba` yields straight R, G, B, A bytes.
        let mut renderer = Self::new_offscreen(
            wgpu::TextureFormat::Rgba8Unorm,
            cache_path,
            vulkan_only,
            font_config,
        )
        .await?;
        // So `read_rgba` works even before the first `begin_frame`, which recreates it at the real frame size.
        if width > 0 && height > 0 {
            renderer.offscreen_output = Some(create_offscreen_texture(
                &renderer.device,
                renderer.surface_format,
                width,
                height,
            ));
        }
        Ok(renderer)
    }

    /// [`new_for_texture`](Self::new_for_texture), blocking, and serialized against every other window's surface lifecycle — the sibling of [`new`](Self::new), and what a caller building one of these beside a live window needs: device and pipeline creation must not overlap another surface's in-flight acquire/present (see `renderer_core::gpu_sync`).
    pub fn for_texture(
        target: wgpu::Texture,
        cache_path: Option<&std::path::Path>,
        vulkan_only: bool,
        font_config: renderer_text::TextShaperConfig,
    ) -> Result<Self, RendererError> {
        let _gpu = renderer_core::gpu_sync::lifecycle_guard();
        pollster::block_on(Self::new_for_texture(
            target,
            cache_path,
            vulkan_only,
            font_config,
        ))
    }

    /// Build a windowless renderer that composes its frames **into a texture the application owns**.
    ///
    /// The mirror of [`crate::gpu::image`]: there the application fills a texture and Telar places it in its frame; here Telar draws its frame inside a picture the application is assembling. Neither direction tells Telar what the rest of the picture is of.
    ///
    /// The frame is *blended* over what the texture already holds, premultiplied-alpha over, so a UI composed with no `clear_color` lands on top of the application's own content rather than erasing it. Rendering at the application's chosen resolution is the point — a UI at 320×180 inside a window that is not, a viewport at half resolution while it is being dragged.
    ///
    /// Requirements on `target`: it must belong to the device Telar is drawing with (see [`crate::gpu::shared`]) and carry `RENDER_ATTACHMENT` usage. Its format decides the format every pipeline here is built against, so it must be renderable and blendable. Drive the renderer with `begin_frame` at the target's own pixel size; [`compose_into`](Self::compose_into) swaps in a new texture when the application resizes.
    pub async fn new_for_texture(
        target: wgpu::Texture,
        cache_path: Option<&std::path::Path>,
        vulkan_only: bool,
        font_config: renderer_text::TextShaperConfig,
    ) -> Result<Self, RendererError> {
        let mut renderer =
            Self::new_offscreen(target.format(), cache_path, vulkan_only, font_config).await?;
        renderer.compose_into(target);
        Ok(renderer)
    }

    /// Swaps the application-owned texture this renderer composes into — how an application resizes it.
    ///
    /// The new texture must match the format the renderer's pipelines were built against (that of the one passed to [`new_for_texture`](Self::new_for_texture)); a different format needs a new renderer.
    pub fn compose_into(&mut self, target: wgpu::Texture) {
        self.app_owned_target = true;
        self.offscreen_output = Some(target);
    }

    // Everything the two offscreen constructors share: no window, no surface, no swapchain. Only the format differs, and with it every pipeline built below.
    async fn new_offscreen(
        surface_format: wgpu::TextureFormat,
        cache_path: Option<&std::path::Path>,
        vulkan_only: bool,
        font_config: renderer_text::TextShaperConfig,
    ) -> Result<Self, RendererError> {
        let backends = if vulkan_only {
            wgpu::Backends::VULKAN
        } else {
            wgpu::Backends::all()
        };
        // The process-wide device is shared; this renderer draws offscreen, with no surface.
        let gpu = shared_gpu(backends).await?;
        let instance = gpu.instance.clone();
        let adapter = gpu.adapter.clone();

        let msaa_samples = if adapter
            .get_texture_format_features(surface_format)
            .flags
            .contains(wgpu::TextureFormatFeatureFlags::MULTISAMPLE_X4)
        {
            4
        } else {
            1
        };
        // Only consumed when configuring a surface, which never happens without one; they still fill the shared `SurfaceConfiguration` built in `reconfigure`.
        let present_mode = wgpu::PresentMode::Fifo;
        let alpha_mode = wgpu::CompositeAlphaMode::Opaque;
        tracing::info!(
            "hw init (offscreen): format={:?} msaa={}",
            surface_format,
            msaa_samples,
        );

        Self::from_parts(
            instance,
            adapter,
            gpu.device.clone(),
            gpu.queue.clone(),
            None,
            None,
            surface_format,
            msaa_samples,
            present_mode,
            alpha_mode,
            cache_path,
            font_config,
        )
        .await
    }
}
