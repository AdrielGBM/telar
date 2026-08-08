use std::collections::HashMap;
use std::collections::VecDeque;
use std::hash::{Hash, Hasher};

use rustc_hash::FxHasher;

use geometry_core::Rect;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use renderer_core::{
    Color, DrawCommand, ImageFilter, RenderBackend, RendererError, expand_fill_layers,
    hash_pod_slice,
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
use crate::primitives::{Viewport, create_viewport_bind_group_layout};

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
    /// The GPU backend has to publish for itself: a census only the CPU backend filled would read "nothing cached"
    /// on a machine rendering entirely on the GPU — silent, and wrong in exactly the case worth looking at.
    fn publish_cache_stats(&self) {
        if !renderer_cache::registry::publish_due() {
            return;
        }
        // One set for the process however many render threads report it, so it publishes under a shared identity
        // rather than each thread's own — otherwise every figure reads once per render thread.
        if let Some(shared) = crate::caches::with_shared(|caches| caches.stats()) {
            renderer_cache::registry::publish_shared("gpu", shared);
        }
    }
}

/// A hardware-accelerated renderer using wgpu. The `W: Send + Sync + 'static` bound is a wgpu requirement for surface creation, not an indication that this renderer is thread-safe. The renderer must only be used on the main thread alongside the reactive runtime; it is not safe to move between threads.
pub struct HardwareRenderer<W: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static> {
    instance: wgpu::Instance,
    // None in headless mode: there is no window/swapchain, so frames render into `offscreen_output` instead of a presented surface.
    surface: Option<Surface<'static>>,
    device: Device,
    queue: Queue,
    config: Option<SurfaceConfiguration>,
    viewport_buffer: wgpu::Buffer,
    viewport_bind_group: wgpu::BindGroup,
    viewport_dirty: bool,
    // True while a non-nested rounded clip is applied via the in-shader SDF path; a rounded PushClip encountered while this is set falls back to a mini-layer.
    shader_clip_active: bool,
    // clip_is_round stack depth (post-push) at which the active shader clip was pushed, used to match its PopClip even when plain scissors are nested inside it.
    shader_clip_depth: usize,
    // Scissor in effect before the active shader clip, restored at the matching PopClip.
    shader_clip_outer_scissor: Option<Rect>,
    // Round-robin pool of (buffer, bind group) pairs reused for per-layer viewport uniforms; avoids a create_buffer_init + create_bind_group driver round-trip per layer each frame. Reset to index 0 at begin_frame.
    viewport_buffer_pool: Vec<(wgpu::Buffer, wgpu::BindGroup)>,
    viewport_buffer_pool_index: usize,
    // Reusable offscreen textures keyed by (width, height, format); reused across frames for backdrop-blur scratch targets to avoid per-frame multi-megabyte allocations.
    texture_pool: Vec<(
        u32,
        u32,
        wgpu::TextureFormat,
        wgpu::Texture,
        wgpu::TextureView,
    )>,
    // Upper bound on cached scratch textures per (width, height, format) in texture_pool.
    max_texture_pool_per_size: usize,
    rect_pipeline: RectPipeline,
    text_pipeline: TextPipeline,
    line_pipeline: LinePipeline,
    image_pipeline: ImagePipeline,
    // Real font ascender/line-height metrics for the default face, queried once at construction so dirty-rect computation does not under-estimate the text region.
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
    // Reusable scratch buffers for merge_opaque_batches so the merge pass allocates nothing per frame.
    merge_out: Vec<DrawStep>,
    merge_zone: Vec<DrawStep>,
    // Reusable scratch buffer for prepare_text glyph layout so shaping allocates nothing per text command.
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
    batch_image_key: Option<(u64, ImageFilter)>,
    batch_image_start: Option<u32>,
    batch_image_bind_group: Option<wgpu::BindGroup>,
    draw_state: renderer_core::DrawState,
    layer_texture_pool: Vec<PooledTexture>,
    shadow_capture_pool: Vec<PooledTexture>,
    // Retained frame-wide shadow instance buffer + bind group, keyed by a hash of all pending shadow instances. Reused across frames so unchanged shadows skip per-frame create_buffer_init + create_bind_group.
    shadow_instances_cache: Option<(u64, wgpu::Buffer, wgpu::BindGroup)>,
    // Resolved layer textures keyed by a hash of their draw commands + layer params. Value is (resolve_texture, resolve_view, pixel_count). Lets unchanged static layers skip their whole render pass and composite directly.
    layer_resolved_cache: HashMap<u64, (wgpu::Texture, wgpu::TextureView, u64)>,
    // LRU eviction order for layer_resolved_cache: front is least-recently-used, back is most-recently-used.
    layer_resolved_cache_order: VecDeque<u64>,
    // Total pixel budget for layer_resolved_cache, set per frame to 4 * width * height.
    layer_cache_pixel_budget: u64,
    // Non-MSAA presentation texture holding the last resolved frame. Used both as the idle-frame fast-path source (blit when commands are unchanged) and as the MSAA resolve target each active frame.
    retained_texture: Option<wgpu::Texture>,
    retained_view: Option<wgpu::TextureView>,
    // Headless-only render target (Some iff `surface` is None). The final frame lands here (via direct draw, MSAA resolve-blit, or copy) so `read_rgba` can copy it back. Sized to the current width/height, recreated on resize in `reconfigure`.
    offscreen_output: Option<wgpu::Texture>,
    prev_commands: Vec<DrawCommand>,
    // ComponentList generation of the last fully rendered frame. Initialized to u64::MAX so the first frame never matches and always renders. Set to the incoming generation after each successful render.
    prev_generation: u64,
    // Generation received from the current begin_frame call; used by render_frame to decide the idle-blit fast path.
    incoming_generation: u64,
    retained_blit_pipeline: crate::composite::CompositePipeline,
    prev_rect_hash: u64,
    prev_text_hash: u64,
    prev_line_hash: u64,
    prev_image_hash: u64,
    // None in headless mode (no backing window). Kept alive otherwise so wgpu's surface stays valid for its lifetime.
    _window: Option<std::sync::Arc<W>>,
}

// Headless render target: RENDER_ATTACHMENT for the final draw/resolve-blit, COPY_SRC for read_rgba, COPY_DST for the msaa_samples==1 copy-to-target path.
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

// Safety: cross-thread transfer via JoinHandle happens before any DrawCommands are processed, so no Rc<> values exist at transfer time (prev_commands starts empty); after joining the renderer lives exclusively on the main thread.
unsafe impl<W: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static> Send
    for HardwareRenderer<W>
{
}

impl<W: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static> Drop for HardwareRenderer<W> {
    fn drop(&mut self) {
        // The caller tearing a window down holds renderer_core::gpu_sync::lifecycle_guard() across the whole
        // renderer+window drop, serializing it against sibling render threads.
        // Release cached layer textures before the device so the driver can free their GPU memory.
        self.layer_resolved_cache.clear();
        // Destroy this window's surface (its VkSurfaceKHR) *before* polling. With a process-shared device wgpu
        // defers surface teardown to the next maintenance on that device; this window's own render thread is
        // already gone, so polling with the surface still alive frees nothing, and the VkSurfaceKHR — which
        // keeps the wl_surface alive — would linger on screen until some *other* window's teardown polls the
        // shared device. Dropping the surface (its `_window` Arc still outlives this body, so the raw handle
        // stays valid) then polling flushes the destruction now. On a lost device `poll` is a fatal wgpu error
        // (a panic); if this Drop runs while unwinding from an earlier render panic that second panic would
        // abort — so catch it and let teardown finish cleanly.
        self.surface = None;
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        }));
    }
}

// One GPU instance/adapter/device/queue shared by every window in the process; per-window renderers each own
// only a `Surface` and hold cloned (Arc) handles to these. Sharing is REQUIRED for multi-window: a separate
// VkInstance/VkDevice per window, destroyed when its window closes, corrupts the shared driver state and
// segfaults a sibling window's in-flight `vkAcquireNextImageKHR` (reproduced on the NVIDIA driver). With a
// shared device, closing a window drops only its swapchain — never a device — which the driver handles fine.
struct SharedGpu {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: Device,
    queue: Queue,
}

static SHARED_GPU: std::sync::OnceLock<SharedGpu> = std::sync::OnceLock::new();

// Keeps a swapchain from being rebuilt while any window is advancing one. A single device is shared by every
// window (see SharedGpu) and the WSI beneath it does not take being driven from several threads at once:
// reconfiguring one window's swapchain while a sibling sits inside its acquire loses the *device*, and a lost
// device is every window at once — which is what a resize, of one window's grip or of every window's scale at
// a time, used to cost.
//
// A read/write lock rather than a mutex because that is the shape of the rule: acquiring and presenting run
// concurrently with each other exactly as wgpu intends, and only a rebuild excludes them. A mutex here made a
// dragged window's per-frame reconfigure serialise every other window's presentation behind it, which is a
// visible stutter across the whole desktop for the sake of a rule that never asked for it.
//
// Taken around each call rather than held across a frame: the acquire path reconfigures on `Lost`, and a read
// guard still held there would deadlock against the write it needs.
static SWAPCHAIN: std::sync::RwLock<()> = std::sync::RwLock::new(());

/// Held while a swapchain is advanced — acquired or presented. Shared: any number of windows at once.
pub(crate) fn swapchain_lock() -> std::sync::RwLockReadGuard<'static, ()> {
    SWAPCHAIN
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Held while a swapchain is built or rebuilt. Exclusive against every window's acquire and present.
pub(crate) fn swapchain_rebuild_lock() -> std::sync::RwLockWriteGuard<'static, ()> {
    // wgpu reports a fatal error as a panic, so the thread holding either guard can die with it. Poisoning
    // would then take down the windows that survived, which is the opposite of the point.
    SWAPCHAIN
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A pipeline built on a scoped thread, or the reason it could not be.
///
/// A panic here is rarely a bug in the pipeline: wgpu reports a fatal device error as a panic, so a device
/// lost by any window arrives as one on every thread that touches it afterwards. Unwrapping would carry that
/// into the caller — the UI thread — and end the process, which is how one dead device cost a whole
/// application rather than the one surface that failed to open.
fn built<T>(handle: std::thread::ScopedJoinHandle<'_, T>) -> Result<T, RendererError> {
    handle
        .join()
        .map_err(|_| RendererError::Backend("a render pipeline could not be built".to_string()))
}

async fn shared_gpu(backends: wgpu::Backends) -> Result<&'static SharedGpu, RendererError> {
    if let Some(gpu) = SHARED_GPU.get() {
        return Ok(gpu);
    }
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    // No `compatible_surface`: the shared adapter serves every window. On a normal single-compositor desktop
    // any window's surface is presentable by the HighPerformance adapter.
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
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

    // A concurrent initializer may have won the race; keep whichever landed first (dropping the loser's
    // handles is safe — no surface is bound to them).
    let _ = SHARED_GPU.set(SharedGpu {
        instance,
        adapter,
        device,
        queue,
    });
    Ok(SHARED_GPU.get().expect("shared GPU just set"))
}

// Hardware scroll-blit-with-clear: seed the offscreen with the previous frame shifted by the scroll delta so a cleared scrolling frame only redraws the exposed band. On by default for the MSAA (desktop) path; set TELAR_HW_SCROLL_BLIT=0 to fall back to a full re-render.
fn hw_scroll_blit_enabled() -> bool {
    use std::sync::OnceLock;
    static V: OnceLock<bool> = OnceLock::new();
    *V.get_or_init(|| std::env::var("TELAR_HW_SCROLL_BLIT").as_deref() != Ok("0"))
}

// Damage tracking with an opaque clear (F1): generalize the scroll-blit-with-clear prime to an
// arbitrary dirty rect so a `clear_color` frame that changed only a small region seeds the offscreen
// with the previous frame (retained_view) and repaints only the dirty scissor instead of the whole
// surface. On by default for the MSAA (desktop) path; set TELAR_HW_DAMAGE=0 to force a full re-render.
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

// Converts a logical scissor rect into clamped physical (x, y, w, h) for set_scissor_rect; width/height >= 1 since wgpu rejects empty scissors.
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
        // Exclusive against every render thread: creating a Vulkan device/surface must not overlap another
        // window's in-flight acquire/present (see renderer_core::gpu_sync).
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
        // Share the process-wide instance/adapter/device (see SharedGpu); this window owns only its surface.
        let gpu = shared_gpu(backends).await?;
        let instance = gpu.instance.clone();
        let adapter = gpu.adapter.clone();

        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| RendererError::Surface(e.to_string()))?;

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = preferred_format(&surface_caps);
        // Android Adreno TBDR GPUs silently drop MSAA samples across render-pass boundaries (StoreOp::Store + LoadOp::Load on multisampled textures yields zeros); force 1 sample on Android.
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
        // On Android always use Fifo: Mailbox on some Adreno/MIUI devices silently drops frames producing a black screen.
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
        // A transparent app needs the compositor to blend its surface (premultiplied alpha); a normal app prefers Opaque. Pick the first mode the surface actually offers from the preference order, falling back to whatever it has.
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
            config,
        )
        .await
    }

    // Surface-independent GPU/device/pipeline construction shared by `new_async` (windowed) and `new_headless` (offscreen). Format/msaa/present/alpha are decided by the caller since only the windowed path can query surface capabilities.
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
        config: HardwareRendererConfig,
    ) -> Result<Self, RendererError> {
        // device/queue are shared process-wide (see SharedGpu) and created there with the IMMEDIATES feature
        // when the adapter supports it; recompute the flag here for the blur pipeline's push-constant path.
        const BLUR_PARAMS_SIZE: u32 = std::mem::size_of::<BlurParams>() as u32;
        let supports_immediates = adapter.features().contains(wgpu::Features::IMMEDIATES)
            && adapter.limits().max_immediate_size >= BLUR_PARAMS_SIZE;

        // Returns None on non-Vulkan backends where pipeline caching is unsupported.
        let (pipeline_cache, cache_file_path) = {
            let adapter_info = adapter.get_info();
            let key = wgpu::util::pipeline_cache_key(&adapter_info);
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

        // Builds this thread's shared caches if this is its first renderer, and yields handles on the one glyph
        // atlas every renderer on the thread samples. Taken before the scope below because the text pipeline is
        // built on a spawned thread, which a thread-local borrow cannot cross.
        let (atlas_bgl, atlas_bind_group) = crate::caches::atlas_handles(
            &device,
            font_config.font.clone(),
            &crate::caches::HardwarePolicies {
                path_tess: config.path_tess,
                shadow: config.shadow,
                image_texture: config.image_texture,
            },
        );

        // Create all Send-safe pipelines in parallel; on Vulkan/Metal this reduces startup from ~8 serial compilations to ~1 critical path, ImagePipeline (Rc<> cache) must be created on this thread, and `pc` must be defined here (not inside the scope closure) so its lifetime covers the spawned threads.
        let pc = pipeline_cache.as_ref();
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
            let t_rect = s.spawn(|| {
                RectPipeline::new(
                    &device,
                    surface_format,
                    &viewport_bind_group_layout,
                    pc,
                    msaa_samples,
                )
            });
            let t_text = s.spawn(|| {
                TextPipeline::new(
                    &device,
                    surface_format,
                    &viewport_bind_group_layout,
                    pc,
                    msaa_samples,
                    &atlas_bgl,
                    atlas_bind_group.clone(),
                )
            });
            let t_line = s.spawn(|| {
                LinePipeline::new(
                    &device,
                    surface_format,
                    &viewport_bind_group_layout,
                    pc,
                    msaa_samples,
                )
            });
            let t_path = s.spawn(|| {
                PathPipeline::new(
                    &device,
                    surface_format,
                    &viewport_bind_group_layout,
                    pc,
                    msaa_samples,
                )
            });
            let t_layer = s.spawn(|| LayerPipeline::new(&device, surface_format, msaa_samples));
            let t_blur =
                s.spawn(|| BlurPipeline::new(&device, surface_format, pc, supports_immediates));
            let t_composite = s.spawn(|| {
                CompositePipeline::new(
                    &device,
                    surface_format,
                    msaa_samples,
                    &viewport_bind_group_layout,
                    pc,
                )
            });
            let t_retained = s.spawn(|| {
                CompositePipeline::new(&device, surface_format, 1, &viewport_bind_group_layout, pc)
            });
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
        // Built after the parallel scope because it needs the shared image layout, which the borrow of the shared
        // set cannot cross a spawned thread to reach.
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

        // Persist pipeline cache data so subsequent startups skip shader compilation.
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

        // Pre-allocate the per-layer viewport buffer/bind-group pool so the common case (few layers per frame) never hits create_buffer_init/create_bind_group during rendering.
        let mut viewport_buffer_pool = Vec::with_capacity(config.viewport_pool_size);
        for _ in 0..config.viewport_pool_size {
            viewport_buffer_pool.push(create_viewport_pool_slot(
                &device,
                &viewport_bind_group_layout,
            ));
        }

        // From the shared shaper, which `atlas_handles` above already built with this font config.
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
            shader_clip_active: false,
            shader_clip_depth: 0,
            shader_clip_outer_scissor: None,
            viewport_buffer_pool,
            viewport_buffer_pool_index: 0,
            texture_pool: Vec::new(),
            max_texture_pool_per_size: config.max_texture_pool_per_size,
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

    /// Rebind the renderer to a new native window after Android resume.
    /// Keeps all GPU resources (device, pipelines, caches, atlas) intact — only the surface is replaced.
    /// The new surface will be configured on the next `begin_frame` call.
    pub fn rebind_surface(&mut self, window: std::sync::Arc<W>) -> Result<(), RendererError> {
        let new_surface = self
            .instance
            .create_surface(window.clone())
            .map_err(|e| RendererError::Surface(e.to_string()))?;
        self.surface = Some(new_surface);
        self._window = Some(window);
        // Force reconfiguration on the next begin_frame (begin_frame handles config.is_none()).
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
        // Buffer copies require each row to start at a COPY_BYTES_PER_ROW_ALIGNMENT (256) boundary; pad the stride and strip the padding after mapping.
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

        let data = slice.get_mapped_range();
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

    // Returns a viewport bind group backed by a pooled uniform buffer holding `viewport`. Reuses a pre-allocated slot via round-robin (writing the new contents in place) when available, otherwise grows the pool with a fresh slot. The returned BindGroup is an Arc-backed clone, so the pool retains ownership of the underlying resources.
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

    // Builds a viewport bind group for the main render pass carrying the given rounded-clip SDF params (clip_rect/radius in logical space). Passing a zero rect and radius restores the unclipped viewport.
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
        // COPY_DST is needed for the non-MSAA (sample_count=1) copy_texture_to_texture path.
        let surface_usage = if self.msaa_samples == 1 {
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_DST
        } else {
            wgpu::TextureUsages::RENDER_ATTACHMENT
        };
        let config = SurfaceConfiguration {
            usage: surface_usage,
            format: self.surface_format,
            width,
            height,
            present_mode: self.present_mode,
            alpha_mode: self.alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        // Headless mode has no surface to configure; the SurfaceConfiguration is still kept in `self.config` so the same `config.is_some()` gating used by the windowed path applies unchanged.
        if let Some(surface) = self.surface.as_ref() {
            let _swapchain = swapchain_rebuild_lock();
            surface.configure(&self.device, &config);
        }
        self.config = Some(config);
        self.viewport_dirty = true;
        // msaa_samples==1: the "resolve" is a texture copy (COPY_SRC), and the idle-blit samples this texture directly (TEXTURE_BINDING) instead of a separate retained copy. A multisample texture cannot be sampled, so TEXTURE_BINDING is added only on the single-sample branch.
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
        // msaa_samples==1: retained texture is the copy destination, so COPY_DST is required.
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
        // Headless: (re)create the offscreen render target at the new size. The windowed path presents to the surface swapchain instead, so it has no offscreen target.
        if self.surface.is_none() {
            self.offscreen_output = Some(create_offscreen_texture(
                &self.device,
                self.surface_format,
                width,
                height,
            ));
        }
        // Invalidate prev_commands on resize so scroll blit is never applied across size changes.
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

// Headless needs no window, so `W` is a pure phantom (`_window` is `None`) — kept generic so the caller picks
// any window type (e.g. the canonical `platform_headless::HeadlessWindow`) without this crate depending on it.
impl<W: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static> HardwareRenderer<W> {
    /// Build a windowless renderer that draws into an offscreen texture instead of a swapchain surface. Read the rendered frame back with [`HardwareRenderer::read_rgba`]. Same font/cache/config parameters as [`HardwareRenderer::new_async`] minus the window; `width`/`height` are the initial physical target size and are re-derived from `begin_frame` on the first frame.
    pub async fn new_headless(
        width: u32,
        height: u32,
        cache_path: Option<&std::path::Path>,
        vulkan_only: bool,
        font_config: renderer_text::TextShaperConfig,
        config: HardwareRendererConfig,
    ) -> Result<Self, RendererError> {
        let backends = if vulkan_only {
            wgpu::Backends::VULKAN
        } else {
            wgpu::Backends::all()
        };
        // Share the process-wide instance/adapter/device (see SharedGpu); headless renders offscreen (no surface).
        let gpu = shared_gpu(backends).await?;
        let instance = gpu.instance.clone();
        let adapter = gpu.adapter.clone();

        // No surface to query, so pick the format the windowed path prefers (pool::preferred_format's first choice). Rgba8Unorm is a mandatory renderable format, so read_rgba yields straight R,G,B,A bytes.
        let surface_format = wgpu::TextureFormat::Rgba8Unorm;
        let msaa_samples = if adapter
            .get_texture_format_features(surface_format)
            .flags
            .contains(wgpu::TextureFormatFeatureFlags::MULTISAMPLE_X4)
        {
            4
        } else {
            1
        };
        // present_mode/alpha_mode are only consumed when configuring a surface, which never happens headless; they still fill the shared SurfaceConfiguration built in reconfigure.
        let present_mode = wgpu::PresentMode::Fifo;
        let alpha_mode = wgpu::CompositeAlphaMode::Opaque;
        tracing::info!(
            "hw init (headless): format={:?} msaa={} target={}x{}",
            surface_format,
            msaa_samples,
            width,
            height,
        );

        let mut renderer = Self::from_parts(
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
            config,
        )
        .await?;

        // Allocate an initial offscreen target so read_rgba works even before the first begin_frame; begin_frame's reconfigure recreates it at the real frame size.
        if width > 0 && height > 0 {
            renderer.offscreen_output = Some(create_offscreen_texture(
                &renderer.device,
                surface_format,
                width,
                height,
            ));
        }

        Ok(renderer)
    }
}
