use std::collections::HashMap;
use std::collections::VecDeque;
use std::hash::{Hash, Hasher};

use rustc_hash::FxHasher;

use geometry_core::Rect;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use renderer_core::{
    Color, DrawCommand, ImageFilter, RenderBackend, RendererError, expand_fill_layers,
    hash_path_style, hash_rect_style, hash_text_style, union_rects,
};

use wgpu::util::DeviceExt;
use wgpu::{Device, Queue, Surface, SurfaceConfiguration};

use crate::blur::{BlurParams, BlurPipeline};
use crate::composite::CompositePipeline;
use crate::primitives::image::{ImageInstance, ImagePipeline};
use crate::primitives::layer::LayerPipeline;
use crate::primitives::line::{LineInstance, LinePipeline};
use crate::primitives::path::{PathFillData, PathPipeline, PathTessCache, PathVertex};
use crate::primitives::rect::{RectInstance, RectPipeline};
use crate::primitives::text::{TextInstance, TextPipeline};
#[cfg(feature = "vello-paths")]
use crate::primitives::vello_renderer::VelloPathRenderer;
use crate::primitives::{Viewport, create_viewport_bgl};

// Prefer Rgba8Unorm: shaders output sRGB-encoded values so the GPU must NOT apply sRGB encoding on write. Bgra8Unorm is the fallback for drivers (e.g. some macOS/DX12 paths) that don't expose Rgba8Unorm.
fn preferred_format(caps: &wgpu::SurfaceCapabilities) -> wgpu::TextureFormat {
    caps.formats
        .iter()
        .find(|f| matches!(f, wgpu::TextureFormat::Rgba8Unorm))
        .or_else(|| {
            caps.formats
                .iter()
                .find(|f| matches!(f, wgpu::TextureFormat::Bgra8Unorm))
        })
        .copied()
        .unwrap_or(caps.formats[0])
}

// Number of viewport buffer/bind-group slots pre-allocated for per-layer uniforms; frames with more concurrent layers fall back to ad-hoc allocations.
const VIEWPORT_POOL_SIZE: usize = 8;
// Upper bound on cached scratch textures per (width, height, format); prevents unbounded GPU memory growth.
const MAX_TEXTURE_POOL_PER_SIZE: usize = 4;

fn create_viewport_pool_slot(
    device: &wgpu::Device,
    viewport_bgl: &wgpu::BindGroupLayout,
) -> (wgpu::Buffer, wgpu::BindGroup) {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("rsx-layer-vp-pool"),
        size: std::mem::size_of::<Viewport>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("rsx-layer-vp-pool-bg"),
        layout: viewport_bgl,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buffer.as_entire_binding(),
        }],
    });
    (buffer, bind_group)
}

// Borrows a scratch texture matching (width, height, format) from the pool, or creates a fresh one on miss. The returned tuple must be handed back via return_pooled_texture once the frame's GPU work is recorded so it can be reused next frame.
fn take_pooled_texture(
    device: &wgpu::Device,
    pool: &mut Vec<(
        u32,
        u32,
        wgpu::TextureFormat,
        wgpu::Texture,
        wgpu::TextureView,
    )>,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    label: &str,
    usage: wgpu::TextureUsages,
) -> (
    u32,
    u32,
    wgpu::TextureFormat,
    wgpu::Texture,
    wgpu::TextureView,
) {
    if let Some(pos) = pool
        .iter()
        .position(|(w, h, f, _, _)| *w == width && *h == height && *f == format)
    {
        return pool.swap_remove(pos);
    }
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (width, height, format, texture, view)
}

// Returns a scratch texture to the pool for reuse, bounded to MAX_TEXTURE_POOL_PER_SIZE entries per (width, height, format) so memory does not grow without limit.
fn return_pooled_texture(
    pool: &mut Vec<(
        u32,
        u32,
        wgpu::TextureFormat,
        wgpu::Texture,
        wgpu::TextureView,
    )>,
    entry: (
        u32,
        u32,
        wgpu::TextureFormat,
        wgpu::Texture,
        wgpu::TextureView,
    ),
) {
    let (w, h, f, _, _) = entry;
    let count = pool
        .iter()
        .filter(|(pw, ph, pf, _, _)| *pw == w && *ph == h && *pf == f)
        .count();
    if count < MAX_TEXTURE_POOL_PER_SIZE {
        pool.push(entry);
    }
}

// Round n up to the nearest multiple of 64 so pool textures are reused across subpixel-layout variations that produce slightly different exact dimensions.
fn bucket_size(n: u32) -> u32 {
    const B: u32 = 64;
    n.div_ceil(B) * B
}

enum DrawStep {
    RectBatch {
        start: u32,
        end: u32,
    },
    TextBatch {
        start: u32,
        end: u32,
    },
    LineBatch {
        start: u32,
        end: u32,
    },
    ImageBatch {
        start: u32,
        end: u32,
        bind_group: wgpu::BindGroup,
        key: (u64, ImageFilter),
    },
    PathDraw {
        index_start: u32,
        index_end: u32,
    },
    SetScissor {
        rect: Option<Rect>,
    },
    BeginLayer {
        msaa_texture: wgpu::Texture,
        msaa_view: wgpu::TextureView,
        resolve_texture: wgpu::Texture,
        resolve_view: wgpu::TextureView,
        viewport_bind_group: wgpu::BindGroup,
        width: u32,
        height: u32,
        offset_x: f32,
        offset_y: f32,
        backdrop_blur: f32,
    },
    EndLayerComposite {
        bind_group: wgpu::BindGroup,
        // Some(hash) when the resolved layer texture should be cached for reuse next frame; None for layers that must not be cached (backdrop blur, round-clip).
        cache_hash: Option<u64>,
        // Outer scissor to apply during the composite blit, so the layer respects parent clip rects (e.g. scroll area). None = full render target.
        scissor: Option<Rect>,
    },
    // Already-cached layer composited directly without a render pass.
    PrerenderedLayer {
        bind_group: wgpu::BindGroup,
        // Outer scissor to apply during the composite blit, so the layer respects parent clip rects (e.g. scroll area). None = full render target.
        scissor: Option<Rect>,
    },
    ShadowPlaceholder {
        op_idx: usize,
    },
    PathShadowPlaceholder {
        op_idx: usize,
    },
    CompositeShadow {
        bind_group: wgpu::BindGroup,
    },
}

struct LayerAccum {
    opacity: f32,
    backdrop_blur: f32,
    begin_step_idx: usize,
    bounds: Option<Rect>,
    // Index into the commands slice just after the PushLayer (start of layer content).
    cmd_start: usize,
    // Instance buffer lengths captured at PushLayer, used to truncate on a cache hit.
    inst_start: u32,
    text_inst_start: u32,
    line_inst_start: u32,
    image_inst_start: u32,
}

// Tries to merge two consecutive same-type batch steps whose instance index ranges are contiguous. Returns Ok(merged) on success, Err((a, b)) if they cannot be merged.
fn try_merge_steps(a: DrawStep, b: DrawStep) -> Result<DrawStep, (DrawStep, DrawStep)> {
    match (a, b) {
        (DrawStep::RectBatch { start: s, end: e1 }, DrawStep::RectBatch { start: s2, end: e2 })
            if e1 == s2 =>
        {
            Ok(DrawStep::RectBatch { start: s, end: e2 })
        }
        (DrawStep::TextBatch { start: s, end: e1 }, DrawStep::TextBatch { start: s2, end: e2 })
            if e1 == s2 =>
        {
            Ok(DrawStep::TextBatch { start: s, end: e2 })
        }
        (DrawStep::LineBatch { start: s, end: e1 }, DrawStep::LineBatch { start: s2, end: e2 })
            if e1 == s2 =>
        {
            Ok(DrawStep::LineBatch { start: s, end: e2 })
        }
        (a, b) => Err((a, b)),
    }
}

#[inline]
fn hash_instances<T: bytemuck::Pod>(data: &[T]) -> u64 {
    let bytes: &[u8] = bytemuck::cast_slice(data);
    let mut hasher = FxHasher::default();
    bytes.hash(&mut hasher);
    hasher.finish()
}

// Hashes a draw-command slice structurally (same approach as the software renderer's hash_commands).
// Uses FxHasher for speed; f32 fields are fed as bit patterns to avoid UB on NaN.
fn hash_draw_commands(commands: &[DrawCommand]) -> u64 {
    use std::sync::Arc;
    let mut h = FxHasher::default();
    commands.len().hash(&mut h);
    for cmd in commands {
        match cmd {
            DrawCommand::Rect { rect, style } => {
                0u8.hash(&mut h);
                rect.x.to_bits().hash(&mut h);
                rect.y.to_bits().hash(&mut h);
                rect.width.to_bits().hash(&mut h);
                rect.height.to_bits().hash(&mut h);
                hash_rect_style(style).hash(&mut h);
            }
            DrawCommand::Text { text, rect, style } => {
                1u8.hash(&mut h);
                text.as_bytes().hash(&mut h);
                rect.x.to_bits().hash(&mut h);
                rect.y.to_bits().hash(&mut h);
                rect.width.to_bits().hash(&mut h);
                rect.height.to_bits().hash(&mut h);
                hash_text_style(style).hash(&mut h);
            }
            DrawCommand::Image { data, rect, filter } => {
                2u8.hash(&mut h);
                data.id.hash(&mut h);
                rect.x.to_bits().hash(&mut h);
                rect.y.to_bits().hash(&mut h);
                rect.width.to_bits().hash(&mut h);
                rect.height.to_bits().hash(&mut h);
                (*filter as u8).hash(&mut h);
            }
            DrawCommand::Line { p1, p2, style } => {
                3u8.hash(&mut h);
                p1.x.to_bits().hash(&mut h);
                p1.y.to_bits().hash(&mut h);
                p2.x.to_bits().hash(&mut h);
                p2.y.to_bits().hash(&mut h);
                style.width.to_bits().hash(&mut h);
            }
            DrawCommand::Path { data, style } => {
                4u8.hash(&mut h);
                (Arc::as_ptr(data) as usize).hash(&mut h);
                hash_path_style(style).hash(&mut h);
            }
            DrawCommand::PushClip { rect, radius } => {
                5u8.hash(&mut h);
                rect.x.to_bits().hash(&mut h);
                rect.y.to_bits().hash(&mut h);
                rect.width.to_bits().hash(&mut h);
                rect.height.to_bits().hash(&mut h);
                radius.top_left.to_bits().hash(&mut h);
                radius.top_right.to_bits().hash(&mut h);
                radius.bottom_right.to_bits().hash(&mut h);
                radius.bottom_left.to_bits().hash(&mut h);
            }
            DrawCommand::PopClip => {
                6u8.hash(&mut h);
            }
            DrawCommand::PushMatrix { matrix } => {
                7u8.hash(&mut h);
                for v in matrix {
                    v.to_bits().hash(&mut h);
                }
            }
            DrawCommand::PopMatrix => {
                8u8.hash(&mut h);
            }
            DrawCommand::PushLayer {
                opacity,
                backdrop_blur,
            } => {
                9u8.hash(&mut h);
                opacity.to_bits().hash(&mut h);
                backdrop_blur.to_bits().hash(&mut h);
            }
            DrawCommand::PopLayer => {
                10u8.hash(&mut h);
            }
            #[cfg(target_os = "android")]
            DrawCommand::AndroidHardwareBufferImage {
                handle,
                rect,
                filter,
                ..
            } => {
                11u8.hash(&mut h);
                handle.hash(&mut h);
                rect.x.to_bits().hash(&mut h);
                rect.y.to_bits().hash(&mut h);
                rect.width.to_bits().hash(&mut h);
                rect.height.to_bits().hash(&mut h);
                (*filter as u8).hash(&mut h);
            }
        }
    }
    h.finish()
}

struct ShadowOp {
    instance_start: u32,
    instance_end: u32,
    sigma: f32,
    tex_w: u32,
    tex_h: u32,
    dest: [f32; 4],
}

struct PathShadowOp {
    index_start: u32,
    index_end: u32,
    sigma: f32,
    tex_w: u32,
    tex_h: u32,
    dest: [f32; 4],
}

#[derive(Hash, PartialEq, Eq, Clone)]
struct ShadowCacheKey {
    instance_start: u32,
    instance_count: u32,
    sigma_bits: u32,
    tex_w: u32,
    tex_h: u32,
    instances_hash: u64,
}

#[derive(Hash, PartialEq, Eq, Clone)]
struct PathShadowCacheKey {
    index_start: u32,
    index_count: u32,
    sigma_bits: u32,
    tex_w: u32,
    tex_h: u32,
    geometry_hash: u64,
}

#[inline]
fn flush_batch(
    pending_steps: &mut Vec<DrawStep>,
    batch_start: &mut Option<u32>,
    vec_len: u32,
    variant: impl Fn(u32, u32) -> DrawStep,
) {
    if let Some(start) = batch_start.take() {
        if vec_len > start {
            pending_steps.push(variant(start, vec_len));
        }
    }
}

#[inline]
fn flush_image_batch(
    pending_steps: &mut Vec<DrawStep>,
    batch_image_start: &mut Option<u32>,
    batch_image_bind_group: &mut Option<wgpu::BindGroup>,
    batch_image_key: &mut Option<(u64, ImageFilter)>,
    pending_image_instances_len: u32,
) {
    if let (Some(start), Some(bind_group), Some(key)) = (
        batch_image_start.take(),
        batch_image_bind_group.take(),
        *batch_image_key,
    ) {
        if pending_image_instances_len > start {
            pending_steps.push(DrawStep::ImageBatch {
                start,
                end: pending_image_instances_len,
                bind_group,
                key,
            });
        }
    }
}

/// A hardware-accelerated renderer using wgpu. The `W: Send + Sync + 'static` bound is a wgpu requirement for surface creation, not an indication that this renderer is thread-safe. The renderer must only be used on the main thread alongside the reactive runtime; it is not safe to move between threads.
pub struct HardwareRenderer<W: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static> {
    instance: wgpu::Instance,
    surface: Surface<'static>,
    device: Device,
    queue: Queue,
    config: Option<SurfaceConfiguration>,
    viewport_buffer: wgpu::Buffer,
    viewport_bind_group: wgpu::BindGroup,
    viewport_dirty: bool,
    // Round-robin pool of (buffer, bind group) pairs reused for per-layer viewport uniforms; avoids a create_buffer_init + create_bind_group driver round-trip per layer each frame. Reset to index 0 at begin_frame.
    viewport_buffer_pool: Vec<(wgpu::Buffer, wgpu::BindGroup)>,
    viewport_buffer_pool_idx: usize,
    // Reusable offscreen textures keyed by (width, height, format); reused across frames for backdrop-blur scratch targets to avoid per-frame multi-megabyte allocations.
    texture_pool: Vec<(
        u32,
        u32,
        wgpu::TextureFormat,
        wgpu::Texture,
        wgpu::TextureView,
    )>,
    rect_pipeline: RectPipeline,
    text_pipeline: TextPipeline,
    line_pipeline: LinePipeline,
    image_pipeline: ImagePipeline,
    text_shaper: renderer_text::TextShaper,
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
    path_pipeline: PathPipeline,
    layer_pipeline: LayerPipeline,
    viewport_bgl: wgpu::BindGroupLayout,
    blur_pipeline: BlurPipeline,
    composite_pipeline: CompositePipeline,
    pending_shadow_instances: Vec<TextInstance>,
    pending_shadow_ops: Vec<ShadowOp>,
    pending_shadow_path_vertices: Vec<PathVertex>,
    pending_shadow_path_indices: Vec<u32>,
    pending_shadow_path_fill_data: Vec<PathFillData>,
    pending_path_shadow_ops: Vec<PathShadowOp>,
    pending_path_vertices: Vec<PathVertex>,
    pending_path_indices: Vec<u32>,
    pending_path_fill_data: Vec<PathFillData>,
    path_tess_cache: PathTessCache,
    // GPU path tessellation via Vello (Finding 4.3). None when the device failed to initialize a Vello renderer; in that case path rendering falls back to the Lyon pipeline even with the feature enabled.
    #[cfg(feature = "vello-paths")]
    vello_path_renderer: Option<VelloPathRenderer>,
    msaa_samples: u32,
    msaa_texture: Option<wgpu::Texture>,
    batch_rect_start: Option<u32>,
    batch_text_start: Option<u32>,
    batch_line_start: Option<u32>,
    batch_image_key: Option<(u64, ImageFilter)>,
    batch_image_start: Option<u32>,
    batch_image_bind_group: Option<wgpu::BindGroup>,
    draw_state: renderer_core::DrawState,
    layer_texture_pool: Vec<(
        wgpu::Texture,
        wgpu::TextureView,
        wgpu::Texture,
        wgpu::TextureView,
        u32,
        u32,
    )>,
    shadow_capture_pool: Vec<(
        wgpu::Texture,
        wgpu::TextureView,
        wgpu::Texture,
        wgpu::TextureView,
        u32,
        u32,
    )>,
    shadow_resolved_cache: HashMap<ShadowCacheKey, (wgpu::Texture, wgpu::TextureView)>,
    // Retained frame-wide shadow instance buffer + bind group, keyed by a hash of all pending shadow instances. Reused across frames so unchanged shadows skip per-frame create_buffer_init + create_bind_group.
    shadow_instances_cache: Option<(u64, wgpu::Buffer, wgpu::BindGroup)>,
    // LRU eviction order for shadow_resolved_cache: front is least-recently-used, back is most-recently-used.
    shadow_resolved_cache_order: VecDeque<ShadowCacheKey>,
    path_shadow_resolved_cache: HashMap<PathShadowCacheKey, (wgpu::Texture, wgpu::TextureView)>,
    // LRU eviction order for path_shadow_resolved_cache: front is least-recently-used, back is most-recently-used.
    path_shadow_resolved_cache_order: VecDeque<PathShadowCacheKey>,
    // Resolved layer textures keyed by a hash of their draw commands + layer params. Value is (resolve_texture, resolve_view, pixel_count). Lets unchanged static layers skip their whole render pass and composite directly.
    layer_resolved_cache: HashMap<u64, (wgpu::Texture, wgpu::TextureView, u64)>,
    // LRU eviction order for layer_resolved_cache: front is least-recently-used, back is most-recently-used.
    layer_resolved_cache_order: VecDeque<u64>,
    // Total pixel budget for layer_resolved_cache, set per frame to 4 * width * height.
    layer_cache_pixel_budget: u64,
    // Non-MSAA presentation texture holding the last resolved frame. Used both as the idle-frame fast-path source (blit when commands are unchanged) and as the MSAA resolve target each active frame.
    retained_texture: Option<wgpu::Texture>,
    retained_view: Option<wgpu::TextureView>,
    prev_commands: Vec<DrawCommand>,
    prev_commands_hash: u64,
    // ComponentList generation of the last fully rendered frame. Initialized to u64::MAX so the first frame never matches and always renders. Set to the incoming generation after each successful render.
    prev_generation: u64,
    // Generation received from the current begin_frame call; used by render_frame to decide the idle-blit fast path.
    incoming_generation: u64,
    retained_blit_pipeline: crate::composite::CompositePipeline,
    prev_rect_hash: u64,
    prev_text_hash: u64,
    prev_line_hash: u64,
    prev_image_hash: u64,
    _window: std::sync::Arc<W>,
}

// Safety: HardwareRenderer is always constructed with prev_commands: Vec::new() (no Rc<> values).
// The cross-thread transfer via JoinHandle happens before any DrawCommands are processed,
// so no Rc<> values exist at transfer time. After joining, the renderer lives exclusively
// on the main thread.
unsafe impl<W: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static> Send
    for HardwareRenderer<W>
{
}

impl<W: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static> Drop for HardwareRenderer<W> {
    fn drop(&mut self) {
        // Wait for all pending GPU work before the device is destroyed so the Vulkan driver
        // frees GEM objects synchronously. Without this, wgpu may defer cleanup to a later
        // maintenance cycle, keeping GPU memory allocated beyond this drop.
        // Release cached layer textures before the device so the driver frees their GEM objects synchronously.
        self.layer_resolved_cache.clear();
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
    }
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
        let in_extra = sb.extra_dirty.map_or(false, |ed| {
            renderer_core::culling::overlaps(
                bounds.x,
                bounds.y,
                bounds.width,
                bounds.height,
                Some(ed),
            )
        });
        if !in_exp && !in_extra {
            return true;
        }
    }
    false
}

impl<W: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static> HardwareRenderer<W> {
    pub fn new(
        window: W,
        cache_path: Option<&std::path::Path>,
        vulkan_only: bool,
        font_config: renderer_text::TextShaperConfig,
    ) -> Result<Self, RendererError> {
        pollster::block_on(Self::new_async(
            window,
            cache_path,
            vulkan_only,
            font_config,
        ))
    }

    pub async fn new_async(
        window: W,
        cache_path: Option<&std::path::Path>,
        vulkan_only: bool,
        font_config: renderer_text::TextShaperConfig,
    ) -> Result<Self, RendererError> {
        let window = std::sync::Arc::new(window);

        let backends = if vulkan_only {
            wgpu::Backends::VULKAN
        } else {
            wgpu::Backends::all()
        };
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });

        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| RendererError::Surface(e.to_string()))?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .map_err(|_| RendererError::Backend("no suitable GPU adapter found".to_string()))?;

        let pipeline_cache_feature = if adapter.features().contains(wgpu::Features::PIPELINE_CACHE)
        {
            wgpu::Features::PIPELINE_CACHE
        } else {
            wgpu::Features::empty()
        };
        const BLUR_PARAMS_SIZE: u32 = std::mem::size_of::<BlurParams>() as u32;
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
                label: Some("rsx-hardware-renderer"),
                required_features: pipeline_cache_feature | immediates_feature,
                required_limits,
                ..Default::default()
            })
            .await
            .map_err(|e| RendererError::Backend(format!("GPU device request failed: {}", e)))?;

        // Returns None on non-Vulkan backends where pipeline caching is unsupported.
        let (pipeline_cache, cache_file_path) = {
            let adapter_info = adapter.get_info();
            let key = wgpu::util::pipeline_cache_key(&adapter_info);
            if let (Some(key), Some(base)) = (key, cache_path) {
                let path = base.join(key);
                let data = std::fs::read(&path).ok();
                let cache = unsafe {
                    device.create_pipeline_cache(&wgpu::PipelineCacheDescriptor {
                        label: Some("rsx-pipeline-cache"),
                        data: data.as_deref(),
                        fallback: true,
                    })
                };
                (Some(cache), Some(path))
            } else {
                (None, None)
            }
        };

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
        // Prefer Opaque for a non-transparent app; Inherit as fallback so the window system decides.
        let alpha_mode = surface_caps
            .alpha_modes
            .iter()
            .find(|&&m| m == wgpu::CompositeAlphaMode::Opaque)
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

        let viewport_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rsx-viewport"),
            size: std::mem::size_of::<Viewport>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let viewport_bgl = create_viewport_bgl(&device);
        let viewport_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rsx-viewport-bg"),
            layout: &viewport_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: viewport_buffer.as_entire_binding(),
            }],
        });

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
                RectPipeline::new(&device, surface_format, &viewport_bgl, pc, msaa_samples)
            });
            let t_text = s.spawn(|| {
                TextPipeline::new(&device, surface_format, &viewport_bgl, pc, msaa_samples)
            });
            let t_line = s.spawn(|| {
                LinePipeline::new(&device, surface_format, &viewport_bgl, pc, msaa_samples)
            });
            let t_path = s.spawn(|| {
                PathPipeline::new(&device, surface_format, &viewport_bgl, pc, msaa_samples)
            });
            let t_layer = s.spawn(|| LayerPipeline::new(&device, surface_format, msaa_samples));
            let t_blur =
                s.spawn(|| BlurPipeline::new(&device, surface_format, pc, supports_immediates));
            let t_composite = s.spawn(|| {
                CompositePipeline::new(&device, surface_format, msaa_samples, &viewport_bgl, pc)
            });
            let t_retained =
                s.spawn(|| CompositePipeline::new(&device, surface_format, 1, &viewport_bgl, pc));
            (
                t_rect.join().unwrap(),
                t_text.join().unwrap(),
                t_line.join().unwrap(),
                t_path.join().unwrap(),
                t_layer.join().unwrap(),
                t_blur.join().unwrap(),
                t_composite.join().unwrap(),
                t_retained.join().unwrap(),
            )
        });
        // ImagePipeline holds an Rc<> cache and is not Send; create after the parallel scope.
        let image_pipeline = ImagePipeline::new(
            &device,
            surface_format,
            &viewport_bgl,
            pipeline_cache.as_ref(),
            msaa_samples,
        );
        let path_tess_cache = PathTessCache::new();

        #[cfg(feature = "vello-paths")]
        let vello_path_renderer = VelloPathRenderer::new(&device);

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

        // Pre-allocate the per-layer viewport buffer/bind-group pool so the common case (<=8 layers per frame) never hits create_buffer_init/create_bind_group during rendering.
        let mut viewport_buffer_pool = Vec::with_capacity(VIEWPORT_POOL_SIZE);
        for _ in 0..VIEWPORT_POOL_SIZE {
            viewport_buffer_pool.push(create_viewport_pool_slot(&device, &viewport_bgl));
        }

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
            viewport_buffer_pool_idx: 0,
            texture_pool: Vec::new(),
            rect_pipeline,
            text_pipeline,
            line_pipeline,
            image_pipeline,
            path_pipeline,
            layer_pipeline,
            viewport_bgl,
            blur_pipeline,
            composite_pipeline,
            pending_shadow_instances: Vec::new(),
            pending_shadow_ops: Vec::new(),
            pending_shadow_path_vertices: Vec::new(),
            pending_shadow_path_indices: Vec::new(),
            pending_shadow_path_fill_data: Vec::new(),
            pending_path_shadow_ops: Vec::new(),
            text_shaper: renderer_text::TextShaper::with_config(font_config),
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
            pending_path_vertices: Vec::new(),
            pending_path_indices: Vec::new(),
            pending_path_fill_data: Vec::new(),
            path_tess_cache,
            #[cfg(feature = "vello-paths")]
            vello_path_renderer,
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
            shadow_resolved_cache: HashMap::new(),
            shadow_instances_cache: None,
            shadow_resolved_cache_order: VecDeque::new(),
            path_shadow_resolved_cache: HashMap::new(),
            path_shadow_resolved_cache_order: VecDeque::new(),
            layer_resolved_cache: HashMap::new(),
            layer_resolved_cache_order: VecDeque::new(),
            layer_cache_pixel_budget: 0,
            retained_texture: None,
            retained_view: None,
            prev_commands: Vec::new(),
            prev_commands_hash: 0,
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
        self.surface = new_surface;
        self._window = window;
        // Force reconfiguration on the next begin_frame (begin_frame handles config.is_none()).
        self.config = None;
        self.viewport_dirty = true;
        Ok(())
    }

    // Returns a viewport bind group backed by a pooled uniform buffer holding `vp`. Reuses a pre-allocated slot via round-robin (writing the new contents in place) when available, otherwise grows the pool with a fresh slot. The returned BindGroup is an Arc-backed clone, so the pool retains ownership of the underlying resources.
    fn take_layer_viewport_bg(&mut self, vp: Viewport) -> wgpu::BindGroup {
        let idx = self.viewport_buffer_pool_idx;
        self.viewport_buffer_pool_idx += 1;
        if idx >= self.viewport_buffer_pool.len() {
            let slot = create_viewport_pool_slot(&self.device, &self.viewport_bgl);
            self.queue.write_buffer(&slot.0, 0, bytemuck::bytes_of(&vp));
            let bg = slot.1.clone();
            self.viewport_buffer_pool.push(slot);
            return bg;
        }
        let (buffer, bind_group) = &self.viewport_buffer_pool[idx];
        self.queue.write_buffer(buffer, 0, bytemuck::bytes_of(&vp));
        bind_group.clone()
    }

    // Rasterizes the frame's accumulated Vello path scene to an offscreen texture and composites it over the surface using the single-sample blit pipeline (which targets the surface format with premultiplied-alpha blending). No-op when nothing was recorded or the Vello renderer is unavailable.
    #[cfg(feature = "vello-paths")]
    fn composite_vello_paths(&mut self, surface_view: &wgpu::TextureView) {
        let width = self.width;
        let height = self.height;
        let Some(vello) = self.vello_path_renderer.as_mut() else {
            return;
        };
        let Some(vello_view) = vello.render(&self.device, &self.queue, width, height) else {
            return;
        };
        let vello_view = vello_view.clone();
        let bind_group = self.retained_blit_pipeline.create_bind_group(
            &self.device,
            &vello_view,
            [
                0.0,
                0.0,
                width as f32 / self.scale_factor,
                height as f32 / self.scale_factor,
            ],
            1.0,
            0.0,
            [1.0, 1.0],
        );
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("rsx-vello-composite-encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rsx-vello-composite"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: surface_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.retained_blit_pipeline.pipeline);
            pass.set_bind_group(0, &self.viewport_bind_group, &[]);
            pass.set_bind_group(1, &bind_group, &[]);
            pass.draw(0..6, 0..1);
        }
        self.queue.submit(std::iter::once(encoder.finish()));
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

        self.surface.configure(&self.device, &config);
        self.config = Some(config);
        self.viewport_dirty = true;
        // msaa_samples==1: the "resolve" is a texture copy, so COPY_SRC is required.
        let msaa_usage = if self.msaa_samples == 1 {
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC
        } else {
            wgpu::TextureUsages::RENDER_ATTACHMENT
        };
        self.msaa_texture = Some(self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rsx-msaa"),
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
            label: Some("rsx-retained"),
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
        // Invalidate prev_commands on resize so scroll blit is never applied across size changes.
        self.prev_commands.clear();
        self.prev_commands_hash = 0;
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
        self.pending_shadow_ops.clear();
        self.pending_shadow_path_vertices.clear();
        self.pending_shadow_path_indices.clear();
        self.pending_shadow_path_fill_data.clear();
        self.pending_path_shadow_ops.clear();
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

    // Stable-sorts RectBatch/TextBatch/LineBatch within flat zones (separated by structural markers), then merges consecutive same-type batches with contiguous index ranges. Reduces 2N draw calls for a list of N items (rect+label) down to 2.
    fn merge_opaque_batches(&mut self) {
        let steps = &mut self.pending_steps;
        let mut out: Vec<DrawStep> = Vec::with_capacity(steps.len());
        let mut zone: Vec<DrawStep> = Vec::new();

        fn flush_zone(zone: &mut Vec<DrawStep>, out: &mut Vec<DrawStep>) {
            if zone.is_empty() {
                return;
            }
            zone.sort_by_key(|s| match s {
                DrawStep::RectBatch { .. } => 0u8,
                DrawStep::LineBatch { .. } => 1,
                DrawStep::TextBatch { .. } => 2,
                _ => 3,
            });
            let mut merged: Option<DrawStep> = None;
            for step in zone.drain(..) {
                match merged.take() {
                    None => merged = Some(step),
                    Some(prev) => match try_merge_steps(prev, step) {
                        Ok(m) => merged = Some(m),
                        Err((a, b)) => {
                            out.push(a);
                            merged = Some(b);
                        }
                    },
                }
            }
            if let Some(last) = merged {
                out.push(last);
            }
        }

        for step in steps.drain(..) {
            match &step {
                DrawStep::RectBatch { .. }
                | DrawStep::TextBatch { .. }
                | DrawStep::LineBatch { .. } => {
                    zone.push(step);
                }
                _ => {
                    flush_zone(&mut zone, &mut out);
                    out.push(step);
                }
            }
        }
        flush_zone(&mut zone, &mut out);
        *steps = out;
    }
}

impl<W: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static> RenderBackend
    for HardwareRenderer<W>
{
    fn begin_frame(
        &mut self,
        width: u32,
        height: u32,
        scale_factor: f32,
        generation: u64,
    ) -> Result<(), RendererError> {
        self.scale_factor = scale_factor;
        self.incoming_generation = generation;
        if width != self.width || height != self.height || self.config.is_none() {
            // Pooled layer textures are sized to the previous surface dimensions and would be unusable at the new size; drop them so we don't leak GPU memory for textures we will never reuse.
            self.layer_texture_pool.clear();
            // Backdrop-blur scratch textures are sized to the old surface; drop them on resize for the same reason.
            self.texture_pool.clear();
            // Cached layer textures are sized to the old surface; their hashes also encode the old dimensions, so drop them on resize.
            self.layer_resolved_cache.clear();
            self.layer_resolved_cache_order.clear();
            self.width = width;
            self.height = height;
            if width > 0 && height > 0 {
                tracing::debug!(
                    "hw begin_frame: reconfigure {}x{} scale={}",
                    width,
                    height,
                    scale_factor
                );
                self.reconfigure(width, height);
            } else {
                tracing::warn!(
                    "hw begin_frame: zero size {}x{}, skipping reconfigure",
                    width,
                    height
                );
            }
        }
        self.layer_cache_pixel_budget = 4 * self.width as u64 * self.height as u64;
        self.clear_pending();
        self.path_tess_cache.begin_frame();
        #[cfg(feature = "vello-paths")]
        if let Some(vello) = self.vello_path_renderer.as_mut() {
            vello.reset();
        }
        self.image_pipeline.begin_frame();
        self.viewport_buffer_pool_idx = 0;
        Ok(())
    }

    fn render_frame(
        &mut self,
        commands: &[DrawCommand],
        clear_color: Option<Color>,
    ) -> Result<(), RendererError> {
        tracing::debug!(
            "hw render_frame: {} commands, clear={}",
            commands.len(),
            clear_color.is_some()
        );
        // Idle-frame fast path: skip full pipeline and blit retained texture when content generation and viewport are unchanged.
        if self.incoming_generation == self.prev_generation
            && self.retained_view.is_some()
            && !self.viewport_dirty
            && self.config.is_some()
            && self.width > 0
            && self.height > 0
        {
            if let Some(retained_view) = self.retained_view.as_ref() {
                let output = match self.surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(t) => t,
                    wgpu::CurrentSurfaceTexture::Suboptimal(t) => {
                        tracing::debug!("hw idle-blit: suboptimal surface");
                        t
                    }
                    wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                        tracing::warn!("hw idle-blit: surface Lost/Outdated, reconfiguring");
                        if let Some(config) = &self.config.clone() {
                            self.surface.configure(&self.device, config);
                        }
                        self.clear_pending();
                        return Ok(());
                    }
                    wgpu::CurrentSurfaceTexture::Timeout => {
                        tracing::warn!("hw idle-blit: Timeout, skipping frame");
                        self.clear_pending();
                        return Ok(());
                    }
                    wgpu::CurrentSurfaceTexture::Occluded => {
                        tracing::warn!("hw idle-blit: Occluded, skipping frame");
                        self.clear_pending();
                        return Ok(());
                    }
                    other => {
                        self.clear_pending();
                        return Err(RendererError::Present(format!("surface error: {other:?}")));
                    }
                };
                let surface_view = output
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                let retained_bg = self.retained_blit_pipeline.create_bind_group(
                    &self.device,
                    retained_view,
                    [
                        0.0,
                        0.0,
                        self.width as f32 / self.scale_factor,
                        self.height as f32 / self.scale_factor,
                    ],
                    1.0,
                    0.0,
                    [1.0, 1.0],
                );
                let mut encoder =
                    self.device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("rsx-idle-blit"),
                        });
                {
                    let mut blit = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("rsx-idle-blit-pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &surface_view,
                            resolve_target: None,
                            depth_slice: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        occlusion_query_set: None,
                        timestamp_writes: None,
                        multiview_mask: None,
                    });
                    blit.set_pipeline(&self.retained_blit_pipeline.pipeline);
                    blit.set_bind_group(0, &self.viewport_bind_group, &[]);
                    blit.set_bind_group(1, &retained_bg, &[]);
                    blit.draw(0..6, 0..1);
                }
                self.queue.submit(std::iter::once(encoder.finish()));
                tracing::debug!("hw idle-blit: presenting");
                output.present();
                self.clear_pending();
                return Ok(());
            }
        }

        self.draw_state.reset();
        // scroll_blit requires LoadOp::Load to work; when clear_color is set the frame is cleared (LoadOp::Clear), so skipping commands would leave cleared pixels instead of correct content.
        let scroll_blit = if clear_color.is_none() {
            renderer_core::dirty::detect_scroll_blit(commands, &self.prev_commands)
        } else {
            None
        };
        // compute_dirty_rect now returns the changed regions as disjoint rects; the hardware path uses a single scissor, so collapse them into their union here.
        let dirty_scissor: Option<Rect> =
            if clear_color.is_none() && scroll_blit.is_none() && !self.prev_commands.is_empty() {
                renderer_core::dirty::compute_dirty_rect(
                    commands,
                    &self.prev_commands,
                    renderer_core::culling::command_visual_rect,
                )
                .and_then(|rects| rects.into_iter().reduce(union_rects))
            } else {
                None
            };
        let mut current_scissor: Option<Rect> = None;
        let mut scissor_layer_stack: Vec<Option<Rect>> = Vec::new(); // saves/restores current_scissor across PushLayer/PopLayer; layers disable frustum culling inside their bounds
        let mut layer_accum_stack: Vec<LayerAccum> = Vec::new();
        // Composite bind_groups for rounded PushClip mini-layers, consumed at the matching PopClip.
        let mut round_clip_composite: Vec<wgpu::BindGroup> = Vec::new();
        // Parallel to draw_state clip stack: true = rounded mini-layer, false = scissor rect.
        let mut clip_is_round: Vec<bool> = Vec::new();
        let layer_blit_stack: Vec<wgpu::BindGroup> = Vec::new();

        let orig_commands = commands;
        let expanded_commands = expand_fill_layers(commands);
        let commands: &[DrawCommand] = expanded_commands.as_deref().unwrap_or(commands);

        for (cmd_idx, cmd) in commands.iter().enumerate() {
            match cmd {
                DrawCommand::Rect { rect, style } => {
                    let rect = *rect;
                    let style = **style;
                    if rect.width <= 0.0
                        || rect.height <= 0.0
                        || (style.fill.is_none() && style.stroke.is_none())
                    {
                        continue;
                    }
                    if let Some(bounds) =
                        renderer_core::culling::command_visual_rect(cmd, self.draw_state.cum_matrix)
                    {
                        if cull_bounds(bounds, current_scissor, dirty_scissor, scroll_blit.as_ref())
                        {
                            continue;
                        }
                        if let Some(accum) = layer_accum_stack.last_mut() {
                            accum.bounds =
                                Some(accum.bounds.map_or(bounds, |b| union_rects(b, bounds)));
                        }
                    }
                    self.flush_text();
                    self.flush_line();
                    self.flush_image();
                    if self.batch_rect_start.is_none() {
                        self.batch_rect_start = Some(self.pending_instances.len() as u32);
                    }
                    let inst = crate::primitives::rect::prepare_rect(
                        rect,
                        &style,
                        self.draw_state.cum_matrix,
                    );
                    self.pending_instances.push(inst);
                }
                DrawCommand::Text { text, rect, style } => {
                    let rect = *rect;
                    let style = **style;
                    if let Some(bounds) =
                        renderer_core::culling::command_visual_rect(cmd, self.draw_state.cum_matrix)
                    {
                        if cull_bounds(bounds, current_scissor, dirty_scissor, scroll_blit.as_ref())
                        {
                            continue;
                        }
                        if let Some(accum) = layer_accum_stack.last_mut() {
                            accum.bounds =
                                Some(accum.bounds.map_or(bounds, |b| union_rects(b, bounds)));
                        }
                    }
                    self.flush_rect();
                    self.flush_line();
                    self.flush_image();
                    let (text_tx, text_ty) = self.draw_state.apply_point(rect.x, rect.y);
                    let (text_tx2, text_ty2) = self
                        .draw_state
                        .apply_point(rect.x + rect.width, rect.y + rect.height);
                    let translated = Rect::new(
                        text_tx,
                        text_ty,
                        (text_tx2 - text_tx).abs(),
                        (text_ty2 - text_ty).abs(),
                    );
                    if let Some(shadow) = style.shadow {
                        self.flush_text();

                        let sigma = renderer_core::blur_sigma(shadow.blur_radius);
                        let sigma_phys = sigma * self.scale_factor;
                        let padding = (sigma * 3.0).ceil() as u32 + 2;
                        let shadow_rect = Rect::new(
                            translated.x + shadow.offset_x,
                            translated.y + shadow.offset_y,
                            translated.width,
                            translated.height,
                        );
                        let origin_x = shadow_rect.x - padding as f32;
                        let origin_y = shadow_rect.y - padding as f32;
                        let tex_w_log = (shadow_rect.width.ceil() as u32 + 2 * padding).max(1);
                        let tex_h_log = (shadow_rect.height.ceil() as u32 + 2 * padding).max(1);
                        let tex_w = (tex_w_log as f32 * self.scale_factor).ceil() as u32;
                        let tex_h = (tex_h_log as f32 * self.scale_factor).ceil() as u32;

                        let shadow_style = renderer_core::TextStyle {
                            paint: renderer_core::Paint::Solid(shadow.color),
                            shadow: None,
                            ..style
                        };
                        let instance_start = self.pending_shadow_instances.len() as u32;
                        crate::primitives::text::prepare_text(
                            &mut self.text_shaper,
                            text,
                            shadow_rect,
                            &shadow_style,
                            self.scale_factor,
                            &mut self.pending_shadow_instances,
                        );
                        let instance_end = self.pending_shadow_instances.len() as u32;
                        for inst in &mut self.pending_shadow_instances[instance_start as usize..] {
                            inst.dest_rect[0] -= origin_x;
                            inst.dest_rect[1] -= origin_y;
                        }

                        self.pending_shadow_ops.push(ShadowOp {
                            instance_start,
                            instance_end,
                            sigma: sigma_phys,
                            tex_w,
                            tex_h,
                            dest: [origin_x, origin_y, tex_w_log as f32, tex_h_log as f32],
                        });
                        self.pending_steps.push(DrawStep::ShadowPlaceholder {
                            op_idx: self.pending_shadow_ops.len() - 1,
                        });
                    }
                    if self.batch_text_start.is_none() {
                        self.batch_text_start = Some(self.pending_text_instances.len() as u32);
                    }
                    crate::primitives::text::prepare_text(
                        &mut self.text_shaper,
                        text,
                        translated,
                        &style,
                        self.scale_factor,
                        &mut self.pending_text_instances,
                    );
                }
                DrawCommand::Line { p1, p2, style } => {
                    if let Some(bounds) =
                        renderer_core::culling::command_visual_rect(cmd, self.draw_state.cum_matrix)
                    {
                        if cull_bounds(bounds, current_scissor, dirty_scissor, scroll_blit.as_ref())
                        {
                            continue;
                        }
                        if let Some(accum) = layer_accum_stack.last_mut() {
                            accum.bounds =
                                Some(accum.bounds.map_or(bounds, |b| union_rects(b, bounds)));
                        }
                    }
                    self.flush_rect();
                    self.flush_text();
                    self.flush_image();
                    if self.batch_line_start.is_none() {
                        self.batch_line_start = Some(self.pending_line_instances.len() as u32);
                    }
                    use geometry_core::Point;
                    let (lx1, ly1) = self.draw_state.apply_point(p1.x, p1.y);
                    let (lx2, ly2) = self.draw_state.apply_point(p2.x, p2.y);
                    let tp1 = Point::new(lx1, ly1);
                    let tp2 = Point::new(lx2, ly2);
                    self.pending_line_instances
                        .push(crate::primitives::line::prepare_line(tp1, tp2, *style));
                }
                DrawCommand::Image { data, rect, filter } => {
                    if let Some(bounds) =
                        renderer_core::culling::command_visual_rect(cmd, self.draw_state.cum_matrix)
                    {
                        if cull_bounds(bounds, current_scissor, dirty_scissor, scroll_blit.as_ref())
                        {
                            continue;
                        }
                        if let Some(accum) = layer_accum_stack.last_mut() {
                            accum.bounds =
                                Some(accum.bounds.map_or(bounds, |b| union_rects(b, bounds)));
                        }
                    }
                    self.flush_rect();
                    self.flush_text();
                    self.flush_line();
                    let key = (data.id, *filter);
                    if self.batch_image_start.is_none() || self.batch_image_key != Some(key) {
                        self.flush_image();
                        self.batch_image_key = Some(key);
                        self.batch_image_start = Some(self.pending_image_instances.len() as u32);
                        self.batch_image_bind_group =
                            Some(self.image_pipeline.get_or_create_bind_group(
                                &self.device,
                                &self.queue,
                                &data,
                                *filter,
                            ));
                    }
                    let (ix1, iy1) = self.draw_state.apply_point(rect.x, rect.y);
                    let (ix2, iy2) = self.draw_state.apply_point(rect.x + rect.width, rect.y);
                    let (ix3, iy3) = self.draw_state.apply_point(rect.x, rect.y + rect.height);
                    let (ix4, iy4) = self
                        .draw_state
                        .apply_point(rect.x + rect.width, rect.y + rect.height);
                    let imin_x = ix1.min(ix2).min(ix3).min(ix4);
                    let imin_y = iy1.min(iy2).min(iy3).min(iy4);
                    let imax_x = ix1.max(ix2).max(ix3).max(ix4);
                    let imax_y = iy1.max(iy2).max(iy3).max(iy4);
                    let translated = Rect::new(imin_x, imin_y, imax_x - imin_x, imax_y - imin_y);
                    self.pending_image_instances
                        .push(crate::primitives::image::prepare_image(translated));
                }
                DrawCommand::Path { data, style } => {
                    let style = **style;
                    if let Some(bounds) =
                        renderer_core::culling::command_visual_rect(cmd, self.draw_state.cum_matrix)
                    {
                        if cull_bounds(bounds, current_scissor, dirty_scissor, scroll_blit.as_ref())
                        {
                            continue;
                        }
                        if let Some(accum) = layer_accum_stack.last_mut() {
                            accum.bounds =
                                Some(accum.bounds.map_or(bounds, |b| union_rects(b, bounds)));
                        }
                    }
                    self.flush_all();

                    if let Some(shadow) = style.shadow {
                        let shadow_fill = style
                            .fill
                            .map(|_| renderer_core::Paint::Solid(shadow.color));
                        let shadow_stroke = style.stroke.map(|s| renderer_core::Stroke {
                            paint: renderer_core::Paint::Solid(shadow.color),
                            ..s
                        });
                        let shadow_style = renderer_core::PathStyle {
                            fill: shadow_fill,
                            stroke: shadow_stroke,
                            shadow: None,
                            fill_rule: style.fill_rule,
                        };

                        let sv_start = self.pending_shadow_path_vertices.len();
                        let si_start = self.pending_shadow_path_indices.len() as u32;
                        crate::primitives::path::prepare_path(
                            &mut self.path_tess_cache,
                            data,
                            &shadow_style,
                            &mut self.pending_shadow_path_vertices,
                            &mut self.pending_shadow_path_indices,
                            &mut self.pending_shadow_path_fill_data,
                        );
                        let si_end = self.pending_shadow_path_indices.len() as u32;

                        if si_end > si_start {
                            let (mut min_x, mut min_y, mut max_x, mut max_y) =
                                (f32::MAX, f32::MAX, f32::NEG_INFINITY, f32::NEG_INFINITY);
                            for v in &self.pending_shadow_path_vertices[sv_start..] {
                                min_x = min_x.min(v.position[0]);
                                min_y = min_y.min(v.position[1]);
                                max_x = max_x.max(v.position[0]);
                                max_y = max_y.max(v.position[1]);
                            }

                            let (wmin_x, wmin_y) = self.draw_state.apply_point(min_x, min_y);
                            let (wmax_x, wmax_y) = self.draw_state.apply_point(max_x, max_y);
                            let world_min_x = wmin_x.min(wmax_x) + shadow.offset_x;
                            let world_min_y = wmin_y.min(wmax_y) + shadow.offset_y;
                            let world_max_x = wmin_x.max(wmax_x) + shadow.offset_x;
                            let world_max_y = wmin_y.max(wmax_y) + shadow.offset_y;

                            let sigma = renderer_core::blur_sigma(shadow.blur_radius);
                            let sigma_phys = sigma * self.scale_factor;
                            let padding = (sigma * 3.0).ceil() as u32 + 2;

                            let origin_x = world_min_x - padding as f32;
                            let origin_y = world_min_y - padding as f32;
                            let tex_w_log =
                                ((world_max_x - world_min_x).ceil() as u32 + 2 * padding).max(1);
                            let tex_h_log =
                                ((world_max_y - world_min_y).ceil() as u32 + 2 * padding).max(1);
                            let tex_w = (tex_w_log as f32 * self.scale_factor).ceil() as u32;
                            let tex_h = (tex_h_log as f32 * self.scale_factor).ceil() as u32;

                            for v in &mut self.pending_shadow_path_vertices[sv_start..] {
                                let (wx, wy) =
                                    self.draw_state.apply_point(v.position[0], v.position[1]);
                                v.position[0] = wx + shadow.offset_x - origin_x;
                                v.position[1] = wy + shadow.offset_y - origin_y;
                            }

                            self.pending_path_shadow_ops.push(PathShadowOp {
                                index_start: si_start,
                                index_end: si_end,
                                sigma: sigma_phys,
                                tex_w,
                                tex_h,
                                dest: [origin_x, origin_y, tex_w_log as f32, tex_h_log as f32],
                            });
                            self.pending_steps.push(DrawStep::PathShadowPlaceholder {
                                op_idx: self.pending_path_shadow_ops.len() - 1,
                            });
                        }
                    }

                    // With the vello-paths feature, record the fill/stroke into the GPU scene and skip Lyon entirely; the accumulated scene is rasterized and composited once at frame end. Shadows above still use the Lyon + blur pipeline.
                    #[cfg(feature = "vello-paths")]
                    let routed_to_vello = if let Some(vello) = self.vello_path_renderer.as_mut() {
                        vello.add_path(data, &style, self.draw_state.cum_matrix);
                        true
                    } else {
                        false
                    };
                    #[cfg(not(feature = "vello-paths"))]
                    let routed_to_vello = false;

                    if !routed_to_vello {
                        let vertex_start = self.pending_path_vertices.len();
                        let index_start = self.pending_path_indices.len() as u32;
                        let fill_data_start = self.pending_path_fill_data.len();
                        crate::primitives::path::prepare_path(
                            &mut self.path_tess_cache,
                            data,
                            &style,
                            &mut self.pending_path_vertices,
                            &mut self.pending_path_indices,
                            &mut self.pending_path_fill_data,
                        );
                        for v in &mut self.pending_path_vertices[vertex_start..] {
                            let (wx, wy) =
                                self.draw_state.apply_point(v.position[0], v.position[1]);
                            v.position[0] = wx;
                            v.position[1] = wy;
                        }
                        for fd in &mut self.pending_path_fill_data[fill_data_start..] {
                            let (gx0, gy0) =
                                self.draw_state.apply_point(fd.grad_p0[0], fd.grad_p0[1]);
                            let (gx1, gy1) =
                                self.draw_state.apply_point(fd.grad_p1[0], fd.grad_p1[1]);
                            fd.grad_p0 = [gx0, gy0];
                            fd.grad_p1 = [gx1, gy1];
                        }
                        let index_end = self.pending_path_indices.len() as u32;
                        if index_end > index_start {
                            self.pending_steps.push(DrawStep::PathDraw {
                                index_start,
                                index_end,
                            });
                        }
                    }
                }
                DrawCommand::PushClip { rect, radius } => {
                    self.flush_all();
                    if radius.is_zero() {
                        let effective = self.draw_state.push_clip(*rect);
                        current_scissor = Some(effective);
                        clip_is_round.push(false);
                        self.pending_steps.push(DrawStep::SetScissor {
                            rect: Some(effective),
                        });
                    } else {
                        // Rounded clip: allocate a mini-layer, draw into it, composite with SDF mask at PopClip.
                        scissor_layer_stack.push(current_scissor);
                        current_scissor = None;
                        self.draw_state.push_clip(*rect);
                        clip_is_round.push(true);
                        let ox = rect.x.floor().max(0.0);
                        let oy = rect.y.floor().max(0.0);
                        let tex_w_log = (rect.width.ceil() as u32).max(1);
                        let tex_h_log = (rect.height.ceil() as u32).max(1);
                        let tex_w = ((tex_w_log as f32 * self.scale_factor).ceil() as u32)
                            .min(self.width.max(1));
                        let tex_h = ((tex_h_log as f32 * self.scale_factor).ceil() as u32)
                            .min(self.height.max(1));
                        let bucket_w = bucket_size(tex_w);
                        let bucket_h = bucket_size(tex_h);
                        let (msaa_texture, msaa_view, resolve_texture, resolve_view) =
                            if let Some(pos) = self
                                .layer_texture_pool
                                .iter()
                                .position(|(_, _, _, _, pw, ph)| *pw == bucket_w && *ph == bucket_h)
                            {
                                let (mt, mv, rt, rv, _, _) = self.layer_texture_pool.remove(pos);
                                (mt, mv, rt, rv)
                            } else {
                                self.layer_pipeline.create_layer_textures(
                                    &self.device,
                                    bucket_w,
                                    bucket_h,
                                )
                            };
                        let layer_vp = Viewport {
                            // Use physical bucket dimensions: to_ndc scales logical coords by scale_factor,
                            // so size must be physical to map [0, logical_w] correctly to NDC [-1, 1].
                            size: [bucket_w as f32, bucket_h as f32],
                            offset: [ox * self.scale_factor, oy * self.scale_factor],
                            scale: self.scale_factor,
                            _pad: 0.0,
                        };
                        let layer_vp_bg = self.take_layer_viewport_bg(layer_vp);
                        let uv_scale = [
                            tex_w as f32 / bucket_w as f32,
                            tex_h as f32 / bucket_h as f32,
                        ];
                        // composite_bg borrows resolve_view before it moves into BeginLayer
                        let composite_bg = self.composite_pipeline.create_bind_group(
                            &self.device,
                            &resolve_view,
                            [ox, oy, tex_w_log as f32, tex_h_log as f32],
                            1.0,
                            radius.top_left,
                            uv_scale,
                        );
                        self.pending_steps.push(DrawStep::BeginLayer {
                            msaa_texture,
                            msaa_view,
                            resolve_texture,
                            resolve_view,
                            viewport_bind_group: layer_vp_bg,
                            width: bucket_w,
                            height: bucket_h,
                            offset_x: ox,
                            offset_y: oy,
                            backdrop_blur: 0.0,
                        });
                        // Pool return for clip layer textures is handled by the EndLayerComposite execution path.
                        round_clip_composite.push(composite_bg);
                    }
                }
                DrawCommand::PopClip => {
                    self.flush_all();
                    if clip_is_round.pop() == Some(true) {
                        let composite_bg = round_clip_composite
                            .pop()
                            .expect("round_clip_composite underflow");
                        self.draw_state.pop_clip();
                        current_scissor = scissor_layer_stack.pop().flatten();
                        self.pending_steps.push(DrawStep::EndLayerComposite {
                            bind_group: composite_bg,
                            // Round-clip layers draw the clip mask into the texture, so their content is not safely cacheable by command hash.
                            cache_hash: None,
                            scissor: current_scissor,
                        });
                        self.pending_steps.push(DrawStep::SetScissor {
                            rect: current_scissor,
                        });
                    } else {
                        let effective = self.draw_state.pop_clip();
                        current_scissor = effective;
                        self.pending_steps
                            .push(DrawStep::SetScissor { rect: effective });
                    }
                }
                DrawCommand::PushMatrix { matrix } => {
                    self.draw_state.push_matrix(*matrix);
                }
                DrawCommand::PopMatrix => {
                    self.draw_state.pop_matrix();
                }
                DrawCommand::PushLayer {
                    opacity,
                    backdrop_blur,
                } => {
                    self.flush_all();
                    // Disable frustum culling inside the layer to avoid incorrect culling by an outer PushClip; save scissor for restore at PopLayer.
                    scissor_layer_stack.push(current_scissor);
                    current_scissor = None;
                    layer_accum_stack.push(LayerAccum {
                        opacity: *opacity,
                        backdrop_blur: *backdrop_blur,
                        begin_step_idx: self.pending_steps.len(),
                        bounds: None,
                        cmd_start: cmd_idx + 1,
                        inst_start: self.pending_instances.len() as u32,
                        text_inst_start: self.pending_text_instances.len() as u32,
                        line_inst_start: self.pending_line_instances.len() as u32,
                        image_inst_start: self.pending_image_instances.len() as u32,
                    });
                }
                DrawCommand::PopLayer => {
                    self.flush_all();
                    current_scissor = scissor_layer_stack.pop().flatten();
                    if let Some(accum) = layer_accum_stack.pop() {
                        let (offset_x, offset_y, tex_w, tex_h, tex_w_log, tex_h_log) =
                            if let Some(b) = accum.bounds {
                                let ox = b.x.floor().max(0.0);
                                let oy = b.y.floor().max(0.0);
                                let wl = (b.width.ceil() as u32).max(1);
                                let hl = (b.height.ceil() as u32).max(1);
                                let wp = ((wl as f32 * self.scale_factor).ceil() as u32)
                                    .min(self.width.max(1));
                                let hp = ((hl as f32 * self.scale_factor).ceil() as u32)
                                    .min(self.height.max(1));
                                (ox, oy, wp, hp, wl, hl)
                            } else {
                                let wl = (self.width as f32 / self.scale_factor).ceil() as u32;
                                let hl = (self.height as f32 / self.scale_factor).ceil() as u32;
                                (0.0, 0.0, self.width.max(1), self.height.max(1), wl, hl)
                            };
                        // Propagate this layer's visual footprint to the parent layer so nested
                        // layers are included in the parent's bounds (and thus its texture size).
                        if let Some(parent) = layer_accum_stack.last_mut() {
                            let footprint =
                                Rect::new(offset_x, offset_y, tex_w_log as f32, tex_h_log as f32);
                            parent.bounds = Some(
                                parent
                                    .bounds
                                    .map_or(footprint, |b| union_rects(b, footprint)),
                            );
                        }
                        // Backdrop-blur layers read framebuffer content, so they are never cacheable.
                        let layer_hash: Option<u64> = if accum.backdrop_blur == 0.0 {
                            use std::hash::{Hash, Hasher};
                            let base = hash_draw_commands(&commands[accum.cmd_start..cmd_idx]);
                            let mut h = FxHasher::default();
                            base.hash(&mut h);
                            accum.opacity.to_bits().hash(&mut h);
                            // Use the unclamped floored world bounds (not offset_x/y which are max'd to 0)
                            // so that different scroll positions with the same clamped offset don't alias
                            // to the same cache entry and produce stale composites.
                            let (hash_bx, hash_by) = accum
                                .bounds
                                .map_or((0.0f32, 0.0f32), |b| (b.x.floor(), b.y.floor()));
                            hash_bx.to_bits().hash(&mut h);
                            hash_by.to_bits().hash(&mut h);
                            tex_w.hash(&mut h);
                            tex_h.hash(&mut h);
                            // Text shaping and rasterization depend on the scale factor; mix it in so a scale change without a resize invalidates stale entries.
                            self.scale_factor.to_bits().hash(&mut h);
                            Some(h.finish())
                        } else {
                            None
                        };
                        let cache_hit =
                            layer_hash.is_some_and(|h| self.layer_resolved_cache.contains_key(&h));
                        let bucket_w = bucket_size(tex_w);
                        let bucket_h = bucket_size(tex_h);
                        if cache_hit {
                            let hash = layer_hash.unwrap();
                            let uv_scale = [
                                tex_w as f32 / bucket_w as f32,
                                tex_h as f32 / bucket_h as f32,
                            ];
                            let bind_group = {
                                let (_, cached_view, _) = &self.layer_resolved_cache[&hash];
                                self.composite_pipeline.create_bind_group(
                                    &self.device,
                                    cached_view,
                                    [offset_x, offset_y, tex_w_log as f32, tex_h_log as f32],
                                    accum.opacity,
                                    0.0,
                                    uv_scale,
                                )
                            };
                            // Refresh LRU position so reused layers are not evicted first.
                            if let Some(pos) = self
                                .layer_resolved_cache_order
                                .iter()
                                .position(|k| *k == hash)
                            {
                                self.layer_resolved_cache_order.remove(pos);
                            }
                            self.layer_resolved_cache_order.push_back(hash);
                            // The layer content emitted DrawSteps and instance data we no longer need; drop them so they neither render nor leave dangling instance ranges.
                            self.pending_steps.truncate(accum.begin_step_idx);
                            self.pending_instances.truncate(accum.inst_start as usize);
                            self.pending_text_instances
                                .truncate(accum.text_inst_start as usize);
                            self.pending_line_instances
                                .truncate(accum.line_inst_start as usize);
                            self.pending_image_instances
                                .truncate(accum.image_inst_start as usize);
                            self.pending_steps.push(DrawStep::PrerenderedLayer {
                                bind_group,
                                scissor: current_scissor,
                            });
                            // Re-apply the outer scissor after the segment boundary. Skip when
                            // current_scissor is None: the new render pass already defaults to the
                            // full target, and emitting (0,0,w,h) inside a nested layer render
                            // pass would use window dimensions on a smaller texture → validation error.
                            if let Some(s) = current_scissor {
                                self.pending_steps
                                    .push(DrawStep::SetScissor { rect: Some(s) });
                            }
                        } else {
                            let (msaa_texture, msaa_view, resolve_texture, resolve_view) =
                                if let Some(pos) = self.layer_texture_pool.iter().position(
                                    |(_, _, _, _, pw, ph)| *pw == bucket_w && *ph == bucket_h,
                                ) {
                                    let (mt, mv, rt, rv, _, _) =
                                        self.layer_texture_pool.remove(pos);
                                    (mt, mv, rt, rv)
                                } else {
                                    self.layer_pipeline.create_layer_textures(
                                        &self.device,
                                        bucket_w,
                                        bucket_h,
                                    )
                                };
                            let layer_vp = Viewport {
                                // Physical bucket dimensions: to_ndc multiplies logical coords by scale_factor,
                                // so using physical size correctly maps logical content into the physical texture.
                                size: [bucket_w as f32, bucket_h as f32],
                                offset: [
                                    offset_x * self.scale_factor,
                                    offset_y * self.scale_factor,
                                ],
                                scale: self.scale_factor,
                                _pad: 0.0,
                            };
                            let layer_vp_bg = self.take_layer_viewport_bg(layer_vp);
                            let uv_scale = [
                                tex_w as f32 / bucket_w as f32,
                                tex_h as f32 / bucket_h as f32,
                            ];
                            // Composite bind group uses window-absolute dest rect in logical pixels; parent viewport (set 0) converts it to NDC.
                            let composite_bg = self.composite_pipeline.create_bind_group(
                                &self.device,
                                &resolve_view,
                                [offset_x, offset_y, tex_w_log as f32, tex_h_log as f32],
                                accum.opacity,
                                0.0,
                                uv_scale,
                            );
                            self.pending_steps.insert(
                                accum.begin_step_idx,
                                DrawStep::BeginLayer {
                                    msaa_texture,
                                    msaa_view,
                                    resolve_texture,
                                    resolve_view,
                                    viewport_bind_group: layer_vp_bg,
                                    width: bucket_w,
                                    height: bucket_h,
                                    offset_x,
                                    offset_y,
                                    backdrop_blur: accum.backdrop_blur,
                                },
                            );
                            self.pending_steps.push(DrawStep::EndLayerComposite {
                                bind_group: composite_bg,
                                // Only cache when no dirty-scissor is active; otherwise the layer's draws may be clipped to the dirty region, leaving a partially-rendered texture.
                                cache_hash: if dirty_scissor.is_none() {
                                    layer_hash
                                } else {
                                    None
                                },
                                scissor: current_scissor,
                            });
                            // Re-apply the outer scissor after the segment boundary. Skip when
                            // current_scissor is None: the new render pass already defaults to the
                            // full target, and emitting (0,0,w,h) inside a nested layer render
                            // pass would use window dimensions on a smaller texture → validation error.
                            if let Some(s) = current_scissor {
                                self.pending_steps
                                    .push(DrawStep::SetScissor { rect: Some(s) });
                            }
                        }
                    }
                    let _ = layer_blit_stack; // suppresses unused variable warning
                }
                #[cfg(target_os = "android")]
                DrawCommand::AndroidHardwareBufferImage { .. } => {}
            }
        }

        self.flush_all();

        let load_op = if let Some(c) = clear_color {
            let c_arr = c.to_array();
            wgpu::LoadOp::Clear(wgpu::Color {
                r: c_arr[0] as f64,
                g: c_arr[1] as f64,
                b: c_arr[2] as f64,
                a: c_arr[3] as f64,
            })
        } else {
            wgpu::LoadOp::Load
        };

        if self.config.is_none() || self.width == 0 || self.height == 0 {
            tracing::warn!(
                "hw render_frame: skipping, config={} w={} h={}",
                self.config.is_some(),
                self.width,
                self.height
            );
            self.clear_pending();
            return Ok(());
        }

        let output = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) => t,
            wgpu::CurrentSurfaceTexture::Suboptimal(t) => {
                tracing::debug!("hw render_frame: suboptimal surface, rendering anyway");
                t
            }
            wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                tracing::warn!(
                    "hw render_frame: surface Lost/Outdated, reconfiguring {}x{}",
                    self.width,
                    self.height
                );
                if let Some(config) = &self.config.clone() {
                    self.surface.configure(&self.device, config);
                }
                self.clear_pending();
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Timeout => {
                tracing::warn!("hw render_frame: surface Timeout, skipping frame");
                self.clear_pending();
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Occluded => {
                tracing::warn!("hw render_frame: surface Occluded, skipping frame");
                self.clear_pending();
                return Ok(());
            }
            other => {
                self.clear_pending();
                return Err(RendererError::Present(format!("surface error: {other:?}")));
            }
        };

        if self.viewport_dirty {
            let viewport = Viewport {
                size: [self.width as f32, self.height as f32],
                offset: [0.0; 2],
                scale: self.scale_factor,
                _pad: 0.0,
            };
            self.queue
                .write_buffer(&self.viewport_buffer, 0, bytemuck::bytes_of(&viewport));
            self.viewport_dirty = false;
        }

        self.text_pipeline
            .sync_atlas(&self.queue, &mut self.text_shaper.atlas);

        if !self.pending_instances.is_empty() {
            let h = hash_instances(&self.pending_instances);
            if h != self.prev_rect_hash {
                self.rect_pipeline
                    .instances
                    .ensure_capacity(&self.device, self.pending_instances.len());
                self.queue.write_buffer(
                    &self.rect_pipeline.instances.instances_buffer,
                    0,
                    bytemuck::cast_slice(&self.pending_instances),
                );
                self.prev_rect_hash = h;
            }
        } else {
            self.prev_rect_hash = 0;
        }

        if !self.pending_text_instances.is_empty() {
            let h = hash_instances(&self.pending_text_instances);
            if h != self.prev_text_hash {
                self.text_pipeline
                    .instances
                    .ensure_capacity(&self.device, self.pending_text_instances.len());
                self.queue.write_buffer(
                    &self.text_pipeline.instances.instances_buffer,
                    0,
                    bytemuck::cast_slice(&self.pending_text_instances),
                );
                self.prev_text_hash = h;
            }
        } else {
            self.prev_text_hash = 0;
        }

        if !self.pending_line_instances.is_empty() {
            let h = hash_instances(&self.pending_line_instances);
            if h != self.prev_line_hash {
                self.line_pipeline
                    .instances
                    .ensure_capacity(&self.device, self.pending_line_instances.len());
                self.queue.write_buffer(
                    &self.line_pipeline.instances.instances_buffer,
                    0,
                    bytemuck::cast_slice(&self.pending_line_instances),
                );
                self.prev_line_hash = h;
            }
        } else {
            self.prev_line_hash = 0;
        }

        if !self.pending_image_instances.is_empty() {
            let h = hash_instances(&self.pending_image_instances);
            if h != self.prev_image_hash {
                self.image_pipeline
                    .instances
                    .ensure_capacity(&self.device, self.pending_image_instances.len());
                self.queue.write_buffer(
                    &self.image_pipeline.instances.instances_buffer,
                    0,
                    bytemuck::cast_slice(&self.pending_image_instances),
                );
                self.prev_image_hash = h;
            }
        } else {
            self.prev_image_hash = 0;
        }

        if !self.pending_path_vertices.is_empty() {
            self.path_pipeline.ensure_capacity(
                &self.device,
                self.pending_path_vertices.len(),
                self.pending_path_indices.len(),
            );
            self.queue.write_buffer(
                &self.path_pipeline.vertex_buffer,
                0,
                bytemuck::cast_slice(&self.pending_path_vertices),
            );
            self.queue.write_buffer(
                &self.path_pipeline.index_buffer,
                0,
                bytemuck::cast_slice(&self.pending_path_indices),
            );
        }

        if !self.pending_path_fill_data.is_empty() {
            self.path_pipeline
                .fill_data
                .ensure_capacity(&self.device, self.pending_path_fill_data.len());
            self.queue.write_buffer(
                &self.path_pipeline.fill_data.buffer,
                0,
                bytemuck::cast_slice(&self.pending_path_fill_data),
            );
        }

        let has_text_shadows =
            !self.pending_shadow_ops.is_empty() && !self.pending_shadow_instances.is_empty();
        let has_path_shadows = !self.pending_path_shadow_ops.is_empty();

        // Single encoder for both the shadow pre-passes and the main pass; wgpu inserts the necessary barriers between render passes, so a separate pre-encoder and extra queue.submit are unnecessary.
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("rsx-encoder"),
            });

        let (shadow_results, path_shadow_results): (
            Vec<Option<wgpu::BindGroup>>,
            Vec<Option<wgpu::BindGroup>>,
        ) = if has_text_shadows || has_path_shadows {
            // Reuse the retained shadow instance buffer + bind group when the instance data is unchanged; otherwise (re)create and cache them. This avoids a create_buffer_init + create_bind_group round-trip every frame for static shadows.
            let shadow_instances_bg_opt = if has_text_shadows {
                let instances_hash = hash_instances(&self.pending_shadow_instances);
                let cache_valid = self
                    .shadow_instances_cache
                    .as_ref()
                    .is_some_and(|(h, _, _)| *h == instances_hash);
                if !cache_valid {
                    let buf = self
                        .device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("rsx-shadow-instances"),
                            contents: bytemuck::cast_slice(&self.pending_shadow_instances),
                            usage: wgpu::BufferUsages::STORAGE,
                        });
                    let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("rsx-shadow-instances-bg"),
                        layout: &self.text_pipeline.instances.instances_bgl,
                        entries: &[wgpu::BindGroupEntry {
                            binding: 0,
                            resource: buf.as_entire_binding(),
                        }],
                    });
                    self.shadow_instances_cache = Some((instances_hash, buf, bg));
                }
                // create_bind_group returns an owned Arc-backed handle, so clone to hand a copy to the draw loop while keeping the cached one.
                self.shadow_instances_cache
                    .as_ref()
                    .map(|(_, _, bg)| bg.clone())
            } else {
                None
            };

            let shadow_path_vb_opt = if has_path_shadows {
                Some(
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("rsx-shadow-path-vb"),
                            contents: bytemuck::cast_slice(&self.pending_shadow_path_vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        }),
                )
            } else {
                None
            };
            let shadow_path_ib_opt = if has_path_shadows {
                Some(
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("rsx-shadow-path-ib"),
                            contents: bytemuck::cast_slice(&self.pending_shadow_path_indices),
                            usage: wgpu::BufferUsages::INDEX,
                        }),
                )
            } else {
                None
            };
            let shadow_path_fd_bg_opt = if has_path_shadows {
                let fd_buf = self
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("rsx-shadow-path-fd"),
                        contents: bytemuck::cast_slice(&self.pending_shadow_path_fill_data),
                        usage: wgpu::BufferUsages::STORAGE,
                    });
                let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("rsx-shadow-path-fd-bg"),
                    layout: &self.path_pipeline.fill_data.bgl,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: fd_buf.as_entire_binding(),
                    }],
                });
                Some(bg)
            } else {
                None
            };

            let mut text_results: Vec<Option<wgpu::BindGroup>> = Vec::new();
            let mut path_results: Vec<Option<wgpu::BindGroup>> = Vec::new();

            if has_text_shadows {
                let shadow_instances_bg = shadow_instances_bg_opt.unwrap();
                for op in &self.pending_shadow_ops {
                    let instance_count = op.instance_end - op.instance_start;
                    let instances_hash = hash_instances(
                        &self.pending_shadow_instances
                            [op.instance_start as usize..op.instance_end as usize],
                    );
                    let key = ShadowCacheKey {
                        instance_start: op.instance_start,
                        instance_count,
                        sigma_bits: op.sigma.to_bits(),
                        tex_w: op.tex_w,
                        tex_h: op.tex_h,
                        instances_hash,
                    };

                    if let Some((_, cached_view)) = self.shadow_resolved_cache.get(&key) {
                        let cbw = bucket_size(op.tex_w);
                        let cbh = bucket_size(op.tex_h);
                        let bg = self.composite_pipeline.create_bind_group(
                            &self.device,
                            cached_view,
                            op.dest,
                            1.0,
                            0.0,
                            [op.tex_w as f32 / cbw as f32, op.tex_h as f32 / cbh as f32],
                        );
                        text_results.push(Some(bg));
                        if let Some(pos) = self
                            .shadow_resolved_cache_order
                            .iter()
                            .position(|k| *k == key)
                        {
                            self.shadow_resolved_cache_order.remove(pos);
                        }
                        self.shadow_resolved_cache_order.push_back(key);
                        continue;
                    }

                    let cap_bucket_w = bucket_size(op.tex_w);
                    let cap_bucket_h = bucket_size(op.tex_h);
                    let (cap_msaa_texture, cap_msaa_view, cap_resolve_texture, cap_resolve_view) =
                        if let Some(pos) = self
                            .shadow_capture_pool
                            .iter()
                            .position(|(_, _, _, _, w, h)| *w == cap_bucket_w && *h == cap_bucket_h)
                        {
                            let (mt, mv, rt, rv, _, _) = self.shadow_capture_pool.remove(pos);
                            (mt, mv, rt, rv)
                        } else {
                            self.layer_pipeline.create_layer_textures(
                                &self.device,
                                cap_bucket_w,
                                cap_bucket_h,
                            )
                        };

                    // Use bucket dimensions: vertices are local to the shadow texture (0-based),
                    // so size must match the physical texture dimensions, not the logical ones.
                    let vp_data: [f32; 6] = [
                        cap_bucket_w as f32,
                        cap_bucket_h as f32,
                        0.0,
                        0.0,
                        self.scale_factor,
                        0.0,
                    ];
                    let vp_buf =
                        self.device
                            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some("rsx-shadow-vp"),
                                contents: bytemuck::bytes_of(&vp_data),
                                usage: wgpu::BufferUsages::UNIFORM,
                            });
                    let shadow_vp_bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("rsx-shadow-vp-bg"),
                        layout: &self.viewport_bgl,
                        entries: &[wgpu::BindGroupEntry {
                            binding: 0,
                            resource: vp_buf.as_entire_binding(),
                        }],
                    });

                    {
                        let cap_draw_view = if self.msaa_samples > 1 {
                            &cap_msaa_view
                        } else {
                            &cap_resolve_view
                        };
                        let cap_resolve_opt = if self.msaa_samples > 1 {
                            Some(&cap_resolve_view)
                        } else {
                            None
                        };
                        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("rsx-shadow-capture"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: cap_draw_view,
                                resolve_target: cap_resolve_opt,
                                depth_slice: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                                    store: if self.msaa_samples > 1 && cap_resolve_opt.is_some() {
                                        wgpu::StoreOp::Discard
                                    } else {
                                        wgpu::StoreOp::Store
                                    },
                                },
                            })],
                            depth_stencil_attachment: None,
                            occlusion_query_set: None,
                            timestamp_writes: None,
                            multiview_mask: None,
                        });
                        pass.set_pipeline(&self.text_pipeline.pipeline);
                        pass.set_bind_group(0, &shadow_vp_bg, &[]);
                        pass.set_bind_group(1, &shadow_instances_bg, &[]);
                        pass.set_bind_group(2, &self.text_pipeline.atlas_bind_group, &[]);
                        pass.draw(0..6, op.instance_start..op.instance_end);
                    }

                    let (blurred_texture, blurred_view) = self.blur_pipeline.apply(
                        &self.device,
                        &mut encoder,
                        &cap_resolve_view,
                        cap_bucket_w,
                        cap_bucket_h,
                        op.sigma,
                    );
                    let shadow_uv_scale = [
                        op.tex_w as f32 / cap_bucket_w as f32,
                        op.tex_h as f32 / cap_bucket_h as f32,
                    ];
                    let bg = self.composite_pipeline.create_bind_group(
                        &self.device,
                        &blurred_view,
                        op.dest,
                        1.0,
                        0.0,
                        shadow_uv_scale,
                    );
                    text_results.push(Some(bg));
                    if self.shadow_resolved_cache.len() >= 128 {
                        if let Some(oldest) = self.shadow_resolved_cache_order.pop_front() {
                            self.shadow_resolved_cache.remove(&oldest);
                        }
                    }
                    self.shadow_resolved_cache_order.push_back(key.clone());
                    self.shadow_resolved_cache
                        .insert(key, (blurred_texture, blurred_view));
                    self.shadow_capture_pool.push((
                        cap_msaa_texture,
                        cap_msaa_view,
                        cap_resolve_texture,
                        cap_resolve_view,
                        cap_bucket_w,
                        cap_bucket_h,
                    ));
                }
            }

            if has_path_shadows {
                let shadow_path_vb = shadow_path_vb_opt.unwrap();
                let shadow_path_ib = shadow_path_ib_opt.unwrap();
                let shadow_path_fd_bg = shadow_path_fd_bg_opt.unwrap();
                for op in &self.pending_path_shadow_ops {
                    let index_count = op.index_end - op.index_start;
                    let geometry_hash = {
                        let verts = &self.pending_shadow_path_vertices;
                        let idxs = &self.pending_shadow_path_indices
                            [op.index_start as usize..op.index_end as usize];
                        let h = hash_instances(verts);
                        let mut hasher = FxHasher::default();
                        h.hash(&mut hasher);
                        hash_instances(idxs).hash(&mut hasher);
                        hasher.finish()
                    };
                    let path_key = PathShadowCacheKey {
                        index_start: op.index_start,
                        index_count,
                        sigma_bits: op.sigma.to_bits(),
                        tex_w: op.tex_w,
                        tex_h: op.tex_h,
                        geometry_hash,
                    };

                    if let Some((_, cached_view)) = self.path_shadow_resolved_cache.get(&path_key) {
                        let cbw = bucket_size(op.tex_w);
                        let cbh = bucket_size(op.tex_h);
                        let bg = self.composite_pipeline.create_bind_group(
                            &self.device,
                            cached_view,
                            op.dest,
                            1.0,
                            0.0,
                            [op.tex_w as f32 / cbw as f32, op.tex_h as f32 / cbh as f32],
                        );
                        path_results.push(Some(bg));
                        if let Some(pos) = self
                            .path_shadow_resolved_cache_order
                            .iter()
                            .position(|k| *k == path_key)
                        {
                            self.path_shadow_resolved_cache_order.remove(pos);
                        }
                        self.path_shadow_resolved_cache_order.push_back(path_key);
                        continue;
                    }

                    let cap_bucket_w = bucket_size(op.tex_w);
                    let cap_bucket_h = bucket_size(op.tex_h);
                    let (cap_msaa_texture, cap_msaa_view, cap_resolve_texture, cap_resolve_view) =
                        if let Some(pos) = self
                            .shadow_capture_pool
                            .iter()
                            .position(|(_, _, _, _, w, h)| *w == cap_bucket_w && *h == cap_bucket_h)
                        {
                            let (mt, mv, rt, rv, _, _) = self.shadow_capture_pool.remove(pos);
                            (mt, mv, rt, rv)
                        } else {
                            self.layer_pipeline.create_layer_textures(
                                &self.device,
                                cap_bucket_w,
                                cap_bucket_h,
                            )
                        };

                    // Use bucket dimensions: vertices are local to the shadow texture (0-based),
                    // so size must match the physical texture dimensions, not the logical ones.
                    let vp_data: [f32; 6] = [
                        cap_bucket_w as f32,
                        cap_bucket_h as f32,
                        0.0,
                        0.0,
                        self.scale_factor,
                        0.0,
                    ];
                    let vp_buf =
                        self.device
                            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some("rsx-shadow-path-vp"),
                                contents: bytemuck::bytes_of(&vp_data),
                                usage: wgpu::BufferUsages::UNIFORM,
                            });
                    let shadow_vp_bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("rsx-shadow-path-vp-bg"),
                        layout: &self.viewport_bgl,
                        entries: &[wgpu::BindGroupEntry {
                            binding: 0,
                            resource: vp_buf.as_entire_binding(),
                        }],
                    });

                    {
                        let cap_draw_view = if self.msaa_samples > 1 {
                            &cap_msaa_view
                        } else {
                            &cap_resolve_view
                        };
                        let cap_resolve_opt = if self.msaa_samples > 1 {
                            Some(&cap_resolve_view)
                        } else {
                            None
                        };
                        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("rsx-shadow-path-capture"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: cap_draw_view,
                                resolve_target: cap_resolve_opt,
                                depth_slice: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                                    store: if self.msaa_samples > 1 && cap_resolve_opt.is_some() {
                                        wgpu::StoreOp::Discard
                                    } else {
                                        wgpu::StoreOp::Store
                                    },
                                },
                            })],
                            depth_stencil_attachment: None,
                            occlusion_query_set: None,
                            timestamp_writes: None,
                            multiview_mask: None,
                        });
                        pass.set_pipeline(&self.path_pipeline.pipeline);
                        pass.set_bind_group(0, &shadow_vp_bg, &[]);
                        pass.set_bind_group(1, &shadow_path_fd_bg, &[]);
                        pass.set_vertex_buffer(0, shadow_path_vb.slice(..));
                        pass.set_index_buffer(shadow_path_ib.slice(..), wgpu::IndexFormat::Uint32);
                        pass.draw_indexed(op.index_start..op.index_end, 0, 0..1);
                    }

                    let (blurred_texture, blurred_view) = self.blur_pipeline.apply(
                        &self.device,
                        &mut encoder,
                        &cap_resolve_view,
                        cap_bucket_w,
                        cap_bucket_h,
                        op.sigma,
                    );
                    let shadow_uv_scale = [
                        op.tex_w as f32 / cap_bucket_w as f32,
                        op.tex_h as f32 / cap_bucket_h as f32,
                    ];
                    let bg = self.composite_pipeline.create_bind_group(
                        &self.device,
                        &blurred_view,
                        op.dest,
                        1.0,
                        0.0,
                        shadow_uv_scale,
                    );
                    path_results.push(Some(bg));
                    if self.path_shadow_resolved_cache.len() >= 128 {
                        if let Some(oldest) = self.path_shadow_resolved_cache_order.pop_front() {
                            self.path_shadow_resolved_cache.remove(&oldest);
                        }
                    }
                    self.path_shadow_resolved_cache_order
                        .push_back(path_key.clone());
                    self.path_shadow_resolved_cache
                        .insert(path_key, (blurred_texture, blurred_view));
                    self.shadow_capture_pool.push((
                        cap_msaa_texture,
                        cap_msaa_view,
                        cap_resolve_texture,
                        cap_resolve_view,
                        cap_bucket_w,
                        cap_bucket_h,
                    ));
                }
            }

            // Shadow passes are recorded into the shared `encoder` and submitted with the main pass; no separate submit here.
            (text_results, path_results)
        } else {
            (Vec::new(), Vec::new())
        };

        let (mut shadow_results, mut path_shadow_results) = (shadow_results, path_shadow_results);
        for step in &mut self.pending_steps {
            match step {
                DrawStep::ShadowPlaceholder { op_idx } => {
                    if let Some(entry) = shadow_results.get_mut(*op_idx) {
                        if let Some(bg) = entry.take() {
                            *step = DrawStep::CompositeShadow { bind_group: bg };
                        }
                    }
                }
                DrawStep::PathShadowPlaceholder { op_idx } => {
                    if let Some(entry) = path_shadow_results.get_mut(*op_idx) {
                        if let Some(bg) = entry.take() {
                            *step = DrawStep::CompositeShadow { bind_group: bg };
                        }
                    }
                }
                _ => {}
            }
        }

        // Image-batching pre-pass: stable-sort each run of consecutive ImageBatch steps by (id, filter) so non-adjacent draws of the same image become adjacent. This is safe for z-order because the reorder is confined to a single run with no intervening non-image steps.
        {
            let steps = &mut self.pending_steps;
            let mut i = 0;
            while i < steps.len() {
                if matches!(steps[i], DrawStep::ImageBatch { .. }) {
                    let mut j = i + 1;
                    while j < steps.len() && matches!(steps[j], DrawStep::ImageBatch { .. }) {
                        j += 1;
                    }
                    if j - i > 1 {
                        // ImageFilter is not Ord; map it to a u8 for sorting.
                        let filter_ord = |f: ImageFilter| match f {
                            ImageFilter::Nearest => 0u8,
                            ImageFilter::Linear => 1u8,
                        };
                        steps[i..j].sort_by(|a, b| {
                            let (ka, kb) = match (a, b) {
                                (
                                    DrawStep::ImageBatch { key: ka, .. },
                                    DrawStep::ImageBatch { key: kb, .. },
                                ) => (*ka, *kb),
                                _ => unreachable!(),
                            };
                            (ka.0, filter_ord(ka.1)).cmp(&(kb.0, filter_ord(kb.1)))
                        });
                    }
                    i = j;
                } else {
                    i += 1;
                }
            }
        }

        self.merge_opaque_batches();

        let surface_view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let msaa_view = self
            .msaa_texture
            .as_ref()
            .ok_or_else(|| {
                RendererError::Backend(
                    "msaa_texture not initialized; call reconfigure first".into(),
                )
            })?
            .create_view(&wgpu::TextureViewDescriptor::default());

        let retained_view = self.retained_view.as_ref().ok_or_else(|| {
            RendererError::Backend("retained_view not initialized; call begin_frame first".into())
        })?;

        {
            let _init = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rsx-main-init"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &msaa_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: load_op,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });
        }

        enum Segment {
            Draw {
                start: usize,
                end: usize,
            },
            BeginLayer {
                msaa_texture: wgpu::Texture,
                msaa_view: wgpu::TextureView,
                resolve_texture: wgpu::Texture,
                resolve_view: wgpu::TextureView,
                viewport_bind_group: wgpu::BindGroup,
                width: u32,
                height: u32,
                offset_x: f32,
                offset_y: f32,
                backdrop_blur: f32,
            },
            EndLayerComposite {
                bind_group: wgpu::BindGroup,
                cache_hash: Option<u64>,
                scissor: Option<Rect>,
            },
            PrerenderedLayer {
                bind_group: wgpu::BindGroup,
                scissor: Option<Rect>,
            },
        }

        let mut steps = std::mem::take(&mut self.pending_steps);
        let mut segments: Vec<Segment> = Vec::new();
        // Walk steps emitting Segment::Draw with index ranges; extract layer-boundary steps in place via std::mem::replace to avoid moving ownership-bearing variants.
        let mut current_start: usize = 0;
        for i in 0..steps.len() {
            let is_boundary = matches!(
                steps[i],
                DrawStep::BeginLayer { .. }
                    | DrawStep::EndLayerComposite { .. }
                    | DrawStep::PrerenderedLayer { .. }
            );
            if !is_boundary {
                continue;
            }
            if i > current_start {
                segments.push(Segment::Draw {
                    start: current_start,
                    end: i,
                });
            }
            let taken = std::mem::replace(&mut steps[i], DrawStep::SetScissor { rect: None });
            match taken {
                DrawStep::BeginLayer {
                    msaa_texture,
                    msaa_view: lmv,
                    resolve_texture,
                    resolve_view: lrv,
                    viewport_bind_group,
                    width,
                    height,
                    offset_x,
                    offset_y,
                    backdrop_blur,
                } => {
                    segments.push(Segment::BeginLayer {
                        msaa_texture,
                        msaa_view: lmv,
                        resolve_texture,
                        resolve_view: lrv,
                        viewport_bind_group,
                        width,
                        height,
                        offset_x,
                        offset_y,
                        backdrop_blur,
                    });
                }
                DrawStep::EndLayerComposite {
                    bind_group,
                    cache_hash,
                    scissor,
                } => {
                    segments.push(Segment::EndLayerComposite {
                        bind_group,
                        cache_hash,
                        scissor,
                    });
                }
                DrawStep::PrerenderedLayer {
                    bind_group,
                    scissor,
                } => {
                    segments.push(Segment::PrerenderedLayer {
                        bind_group,
                        scissor,
                    });
                }
                _ => unreachable!(),
            }
            current_start = i + 1;
        }
        if current_start < steps.len() {
            segments.push(Segment::Draw {
                start: current_start,
                end: steps.len(),
            });
        }

        let mut layer_stack: Vec<(
            wgpu::Texture,
            wgpu::TextureView, // msaa view (render target)
            wgpu::Texture,
            wgpu::TextureView, // resolve view
            wgpu::BindGroup,   // per-layer viewport bind group
            u32,               // layer texture width
            u32,               // layer texture height
        )> = Vec::new();

        // Backdrop-blur scratch textures borrowed from texture_pool this frame. Held until after submit so the same texture is never reused within one encoder (which would alias reads and writes); returned to the pool below.
        let mut frame_scratch_textures: Vec<(
            u32,
            u32,
            wgpu::TextureFormat,
            wgpu::Texture,
            wgpu::TextureView,
        )> = Vec::new();

        // Marks draw segments preceding EndLayerComposite to inline MSAA resolve into the drawing pass, skipping the dedicated resolve pass.
        let mut inline_resolve_targets: Vec<bool> = vec![false; segments.len()];
        for i in 0..segments.len() {
            if let (Segment::Draw { .. }, Some(Segment::EndLayerComposite { .. })) =
                (&segments[i], segments.get(i + 1))
            {
                inline_resolve_targets[i] = true;
            }
        }

        let mut endlayer_resolve_done: Vec<bool> = vec![false; segments.len()];
        for i in 0..segments.len() {
            if matches!(segments[i], Segment::EndLayerComposite { .. })
                && i > 0
                && inline_resolve_targets[i - 1]
            {
                endlayer_resolve_done[i] = true;
            }
        }

        for (seg_idx, segment) in segments.into_iter().enumerate() {
            match segment {
                Segment::Draw { start, end } => {
                    let draw_steps = &steps[start..end];
                    let inline_resolve = inline_resolve_targets[seg_idx];
                    let attach_view: &wgpu::TextureView =
                        if let Some((_, lmv, _, lrv, _, _, _)) = layer_stack.last() {
                            if self.msaa_samples > 1 { lmv } else { lrv }
                        } else {
                            &msaa_view
                        };
                    let resolve_view_opt: Option<&wgpu::TextureView> =
                        if inline_resolve && self.msaa_samples > 1 {
                            layer_stack.last().map(|(_, _, _, rv, _, _, _)| rv)
                        } else {
                            None
                        };

                    let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("rsx-render-pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: attach_view,
                            resolve_target: resolve_view_opt,
                            depth_slice: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Load,
                                store: if resolve_view_opt.is_some() {
                                    wgpu::StoreOp::Discard // MSAA samples not needed after inline resolve
                                } else {
                                    wgpu::StoreOp::Store
                                },
                            },
                        })],
                        depth_stencil_attachment: None,
                        occlusion_query_set: None,
                        timestamp_writes: None,
                        multiview_mask: None,
                    });

                    let active_vp_bg = if let Some((_, _, _, _, vp_bg, _, _)) = layer_stack.last() {
                        vp_bg
                    } else {
                        &self.viewport_bind_group
                    };
                    render_pass.set_bind_group(0, active_vp_bg, &[]);

                    for step in draw_steps {
                        match step {
                            DrawStep::RectBatch { start, end } => {
                                render_pass.set_pipeline(&self.rect_pipeline.pipeline);
                                render_pass.set_bind_group(
                                    1,
                                    &self.rect_pipeline.instances.instances_bind_group,
                                    &[],
                                );
                                render_pass.draw(0..6, *start..*end);
                            }
                            DrawStep::TextBatch { start, end } => {
                                render_pass.set_pipeline(&self.text_pipeline.pipeline);
                                render_pass.set_bind_group(
                                    1,
                                    &self.text_pipeline.instances.instances_bind_group,
                                    &[],
                                );
                                render_pass.set_bind_group(
                                    2,
                                    &self.text_pipeline.atlas_bind_group,
                                    &[],
                                );
                                render_pass.draw(0..6, *start..*end);
                            }
                            DrawStep::LineBatch { start, end } => {
                                render_pass.set_pipeline(&self.line_pipeline.pipeline);
                                render_pass.set_bind_group(
                                    1,
                                    &self.line_pipeline.instances.instances_bind_group,
                                    &[],
                                );
                                render_pass.draw(0..6, *start..*end);
                            }
                            DrawStep::ImageBatch {
                                start,
                                end,
                                bind_group,
                                key: _,
                            } => {
                                render_pass.set_pipeline(&self.image_pipeline.pipeline);
                                render_pass.set_bind_group(
                                    1,
                                    &self.image_pipeline.instances.instances_bind_group,
                                    &[],
                                );
                                render_pass.set_bind_group(2, bind_group, &[]);
                                render_pass.draw(0..6, *start..*end);
                            }
                            DrawStep::PathDraw {
                                index_start,
                                index_end,
                            } => {
                                render_pass.set_pipeline(&self.path_pipeline.pipeline);
                                render_pass.set_bind_group(
                                    1,
                                    &self.path_pipeline.fill_data.bind_group,
                                    &[],
                                );
                                render_pass.set_vertex_buffer(
                                    0,
                                    self.path_pipeline.vertex_buffer.slice(..),
                                );
                                render_pass.set_index_buffer(
                                    self.path_pipeline.index_buffer.slice(..),
                                    wgpu::IndexFormat::Uint32,
                                );
                                render_pass.draw_indexed(*index_start..*index_end, 0, 0..1);
                            }
                            DrawStep::SetScissor { rect } => {
                                let clipped_rect = match (*rect, dirty_scissor) {
                                    (r, None) => r,
                                    (None, Some(ds)) => Some(ds),
                                    (Some(r), Some(ds)) => r.intersect(ds).or(Some(r)),
                                };
                                match clipped_rect {
                                    None => {
                                        render_pass.set_scissor_rect(0, 0, self.width, self.height);
                                    }
                                    Some(r) => {
                                        let sf = self.scale_factor;
                                        let x = ((r.x * sf).max(0.0).floor() as u32)
                                            .min(self.width.saturating_sub(1));
                                        let y = ((r.y * sf).max(0.0).floor() as u32)
                                            .min(self.height.saturating_sub(1));
                                        let right =
                                            (((r.x + r.width) * sf).ceil() as u32).min(self.width);
                                        let bottom = (((r.y + r.height) * sf).ceil() as u32)
                                            .min(self.height);
                                        let w = right
                                            .saturating_sub(x)
                                            .max(1)
                                            .min(self.width.saturating_sub(x));
                                        let h = bottom
                                            .saturating_sub(y)
                                            .max(1)
                                            .min(self.height.saturating_sub(y));
                                        render_pass.set_scissor_rect(x, y, w, h);
                                    }
                                }
                            }
                            DrawStep::CompositeShadow { bind_group } => {
                                render_pass.set_pipeline(&self.composite_pipeline.pipeline);
                                render_pass.set_bind_group(1, bind_group, &[]);
                                render_pass.draw(0..6, 0..1);
                            }
                            DrawStep::ShadowPlaceholder { .. }
                            | DrawStep::PathShadowPlaceholder { .. } => {}
                            DrawStep::BeginLayer { .. }
                            | DrawStep::EndLayerComposite { .. }
                            | DrawStep::PrerenderedLayer { .. } => {
                                unreachable!("layer boundaries are split into segments")
                            }
                        }
                    }
                }

                Segment::BeginLayer {
                    msaa_texture,
                    msaa_view: layer_msaa_view,
                    resolve_texture,
                    resolve_view,
                    viewport_bind_group,
                    width,
                    height,
                    offset_x,
                    offset_y,
                    backdrop_blur,
                } => {
                    {
                        let clear_target = if self.msaa_samples > 1 {
                            &layer_msaa_view
                        } else {
                            &resolve_view
                        };
                        let _clear = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("rsx-layer-clear"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: clear_target,
                                resolve_target: None,
                                depth_slice: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(wgpu::Color {
                                        r: 0.0,
                                        g: 0.0,
                                        b: 0.0,
                                        a: 0.0,
                                    }),
                                    store: wgpu::StoreOp::Store,
                                },
                            })],
                            depth_stencil_attachment: None,
                            occlusion_query_set: None,
                            timestamp_writes: None,
                            multiview_mask: None,
                        });
                    }

                    if backdrop_blur > 0.0 {
                        // Always sample from the root (main) MSAA for backdrop blur. Any layers
                        // above this point in the stack (e.g. a rounded-clip mini-layer) are
                        // transparent at this moment — blurring their content would yield nothing.
                        // The root MSAA has the fully-rendered app content that the blur should sample.
                        let (parent_w, parent_h) = (self.width, self.height);
                        let parent_msaa_view: &wgpu::TextureView = &msaa_view;

                        // Superset usage so any pooled texture of this size/format can serve as either the resolve target or the crop destination interchangeably.
                        let scratch_usage = wgpu::TextureUsages::RENDER_ATTACHMENT
                            | wgpu::TextureUsages::COPY_SRC
                            | wgpu::TextureUsages::COPY_DST
                            | wgpu::TextureUsages::TEXTURE_BINDING;
                        let temp_resolve_entry = take_pooled_texture(
                            &self.device,
                            &mut self.texture_pool,
                            parent_w.max(1),
                            parent_h.max(1),
                            self.surface_format,
                            "rsx-backdrop-resolve",
                            scratch_usage,
                        );
                        let temp_resolve = &temp_resolve_entry.3;
                        let temp_resolve_view = &temp_resolve_entry.4;

                        if self.msaa_samples > 1 {
                            let _resolve = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                label: Some("rsx-backdrop-parent-resolve"),
                                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                    view: parent_msaa_view,
                                    resolve_target: Some(temp_resolve_view),
                                    depth_slice: None,
                                    ops: wgpu::Operations {
                                        load: wgpu::LoadOp::Load,
                                        // Store: the parent MSAA is still needed after this resolve
                                        // so EndLayerComposite can load it to composite the layer on top.
                                        // Discard here caused a black screen on immediate-mode GPUs (desktop).
                                        store: wgpu::StoreOp::Store,
                                    },
                                })],
                                depth_stencil_attachment: None,
                                occlusion_query_set: None,
                                timestamp_writes: None,
                                multiview_mask: None,
                            });
                        } else {
                            encoder.copy_texture_to_texture(
                                wgpu::TexelCopyTextureInfo {
                                    texture: self.msaa_texture.as_ref().unwrap(),
                                    mip_level: 0,
                                    origin: wgpu::Origin3d::ZERO,
                                    aspect: wgpu::TextureAspect::All,
                                },
                                wgpu::TexelCopyTextureInfo {
                                    texture: temp_resolve,
                                    mip_level: 0,
                                    origin: wgpu::Origin3d::ZERO,
                                    aspect: wgpu::TextureAspect::All,
                                },
                                wgpu::Extent3d {
                                    width: parent_w,
                                    height: parent_h,
                                    depth_or_array_layers: 1,
                                },
                            );
                        }

                        let ox_px = offset_x.floor().max(0.0) as u32;
                        let oy_px = offset_y.floor().max(0.0) as u32;
                        let crop_w = width.min(parent_w.saturating_sub(ox_px));
                        let crop_h = height.min(parent_h.saturating_sub(oy_px));

                        let cropped_entry = take_pooled_texture(
                            &self.device,
                            &mut self.texture_pool,
                            crop_w.max(1),
                            crop_h.max(1),
                            self.surface_format,
                            "rsx-backdrop-crop",
                            scratch_usage,
                        );
                        let cropped = &cropped_entry.3;
                        let cropped_view = &cropped_entry.4;

                        if crop_w > 0 && crop_h > 0 {
                            encoder.copy_texture_to_texture(
                                wgpu::TexelCopyTextureInfo {
                                    texture: temp_resolve,
                                    mip_level: 0,
                                    origin: wgpu::Origin3d {
                                        x: ox_px,
                                        y: oy_px,
                                        z: 0,
                                    },
                                    aspect: wgpu::TextureAspect::All,
                                },
                                wgpu::TexelCopyTextureInfo {
                                    texture: cropped,
                                    mip_level: 0,
                                    origin: wgpu::Origin3d::ZERO,
                                    aspect: wgpu::TextureAspect::All,
                                },
                                wgpu::Extent3d {
                                    width: crop_w,
                                    height: crop_h,
                                    depth_or_array_layers: 1,
                                },
                            );
                        }

                        let (_blurred_tex, blurred_view) = self.blur_pipeline.apply(
                            &self.device,
                            &mut encoder,
                            cropped_view,
                            crop_w.max(1),
                            crop_h.max(1),
                            backdrop_blur,
                        );

                        let backdrop_bg = self.composite_pipeline.create_bind_group(
                            &self.device,
                            &blurred_view,
                            [offset_x, offset_y, crop_w as f32, crop_h as f32],
                            1.0,
                            0.0,
                            [1.0, 1.0],
                        );
                        {
                            let backdrop_target = if self.msaa_samples > 1 {
                                &layer_msaa_view
                            } else {
                                &resolve_view
                            };
                            let mut backdrop_pass =
                                encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                    label: Some("rsx-backdrop-composite"),
                                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                        view: backdrop_target,
                                        resolve_target: None,
                                        depth_slice: None,
                                        ops: wgpu::Operations {
                                            load: wgpu::LoadOp::Load,
                                            store: wgpu::StoreOp::Store,
                                        },
                                    })],
                                    depth_stencil_attachment: None,
                                    occlusion_query_set: None,
                                    timestamp_writes: None,
                                    multiview_mask: None,
                                });
                            backdrop_pass.set_pipeline(&self.composite_pipeline.pipeline);
                            backdrop_pass.set_bind_group(0, &viewport_bind_group, &[]);
                            backdrop_pass.set_bind_group(1, &backdrop_bg, &[]);
                            backdrop_pass.draw(0..6, 0..1);
                        }
                        // Hold these scratch textures until after submit; returning them to the pool now would let a later layer in this same encoder reuse and overwrite them before the GPU reads them.
                        frame_scratch_textures.push(temp_resolve_entry);
                        frame_scratch_textures.push(cropped_entry);
                    }

                    layer_stack.push((
                        msaa_texture,
                        layer_msaa_view,
                        resolve_texture,
                        resolve_view,
                        viewport_bind_group,
                        width,
                        height,
                    ));
                }

                Segment::EndLayerComposite {
                    bind_group,
                    cache_hash,
                    scissor,
                } => {
                    let (l_msaa_tex, l_msaa_view, l_resolve_tex, l_resolve_view, _, lw, lh) =
                        layer_stack
                            .pop()
                            .expect("layer_stack underflow on EndLayerComposite");

                    // When msaa_samples==1, draws already targeted resolve_view directly so no resolve pass is needed.
                    if !endlayer_resolve_done[seg_idx] && self.msaa_samples > 1 {
                        let _resolve = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("rsx-layer-resolve"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: &l_msaa_view,
                                resolve_target: Some(&l_resolve_view),
                                depth_slice: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Load,
                                    store: wgpu::StoreOp::Discard, // MSAA samples not needed after resolve
                                },
                            })],
                            depth_stencil_attachment: None,
                            occlusion_query_set: None,
                            timestamp_writes: None,
                            multiview_mask: None,
                        });
                    }

                    // When msaa_samples==1 (Android) draws target the resolve view (tuple index 3), not the MSAA view (index 1); using the wrong view causes composited content to land on a texture the outer layer never reads, making nested layers disappear.
                    let parent_view: &wgpu::TextureView =
                        if let Some((_, lmv, _, lrv, _, _, _)) = layer_stack.last() {
                            if self.msaa_samples > 1 { lmv } else { lrv }
                        } else {
                            &msaa_view
                        };

                    // composite_pipeline must be used here (not layer_pipeline): its BGL expects viewport at set 0 and composite params at set 1, incompatible with layer_pipeline's single-set layout.
                    let parent_vp_bg: &wgpu::BindGroup =
                        if let Some((_, _, _, _, vp_bg, _, _)) = layer_stack.last() {
                            vp_bg
                        } else {
                            &self.viewport_bind_group
                        };

                    {
                        let mut blit = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("rsx-layer-blit"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: parent_view,
                                resolve_target: None,
                                depth_slice: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Load,
                                    store: wgpu::StoreOp::Store,
                                },
                            })],
                            depth_stencil_attachment: None,
                            occlusion_query_set: None,
                            timestamp_writes: None,
                            multiview_mask: None,
                        });
                        blit.set_pipeline(&self.composite_pipeline.pipeline);
                        blit.set_bind_group(0, parent_vp_bg, &[]);
                        blit.set_bind_group(1, &bind_group, &[]);
                        if let Some(s) = scissor {
                            let sf = self.scale_factor;
                            let x = ((s.x * sf).max(0.0).floor() as u32)
                                .min(self.width.saturating_sub(1));
                            let y = ((s.y * sf).max(0.0).floor() as u32)
                                .min(self.height.saturating_sub(1));
                            let right = (((s.x + s.width) * sf).ceil() as u32).min(self.width);
                            let bottom = (((s.y + s.height) * sf).ceil() as u32).min(self.height);
                            let w = right
                                .saturating_sub(x)
                                .max(1)
                                .min(self.width.saturating_sub(x));
                            let h = bottom
                                .saturating_sub(y)
                                .max(1)
                                .min(self.height.saturating_sub(y));
                            blit.set_scissor_rect(x, y, w, h);
                        }
                        blit.draw(0..6, 0..1);
                    }

                    if let Some(hash) = cache_hash {
                        // Retain the resolved texture so the next frame can composite it directly. The MSAA half is not cacheable (it is consumed by the resolve), so it drops instead of returning to the pool.
                        let pixel_count = lw as u64 * lh as u64;
                        self.layer_resolved_cache
                            .insert(hash, (l_resolve_tex, l_resolve_view, pixel_count));
                        self.layer_resolved_cache_order.push_back(hash);
                        let mut total_pixels: u64 = self
                            .layer_resolved_cache
                            .values()
                            .map(|(_, _, px)| *px)
                            .sum();
                        while total_pixels > self.layer_cache_pixel_budget {
                            match self.layer_resolved_cache_order.pop_front() {
                                Some(oldest) if oldest == hash => {
                                    // Never evict the entry we just inserted; if it alone exceeds the budget keep it for this frame.
                                    self.layer_resolved_cache_order.push_front(oldest);
                                    break;
                                }
                                Some(oldest) => {
                                    if let Some((_, _, px)) =
                                        self.layer_resolved_cache.remove(&oldest)
                                    {
                                        total_pixels = total_pixels.saturating_sub(px);
                                    }
                                }
                                None => break,
                            }
                        }
                    } else {
                        self.layer_texture_pool.push((
                            l_msaa_tex,
                            l_msaa_view,
                            l_resolve_tex,
                            l_resolve_view,
                            lw,
                            lh,
                        ));
                    }
                }

                Segment::PrerenderedLayer {
                    bind_group,
                    scissor,
                } => {
                    // Composite the cached layer texture onto the current target (parent layer or surface) without rendering the layer content.
                    let parent_view: &wgpu::TextureView =
                        if let Some((_, lmv, _, lrv, _, _, _)) = layer_stack.last() {
                            if self.msaa_samples > 1 { lmv } else { lrv }
                        } else {
                            &msaa_view
                        };
                    let parent_vp_bg: &wgpu::BindGroup =
                        if let Some((_, _, _, _, vp_bg, _, _)) = layer_stack.last() {
                            vp_bg
                        } else {
                            &self.viewport_bind_group
                        };
                    let mut blit = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("rsx-prerendered-layer-blit"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: parent_view,
                            resolve_target: None,
                            depth_slice: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Load,
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        occlusion_query_set: None,
                        timestamp_writes: None,
                        multiview_mask: None,
                    });
                    blit.set_pipeline(&self.composite_pipeline.pipeline);
                    blit.set_bind_group(0, parent_vp_bg, &[]);
                    blit.set_bind_group(1, &bind_group, &[]);
                    if let Some(s) = scissor {
                        let sf = self.scale_factor;
                        let x =
                            ((s.x * sf).max(0.0).floor() as u32).min(self.width.saturating_sub(1));
                        let y =
                            ((s.y * sf).max(0.0).floor() as u32).min(self.height.saturating_sub(1));
                        let right = (((s.x + s.width) * sf).ceil() as u32).min(self.width);
                        let bottom = (((s.y + s.height) * sf).ceil() as u32).min(self.height);
                        let w = right
                            .saturating_sub(x)
                            .max(1)
                            .min(self.width.saturating_sub(x));
                        let h = bottom
                            .saturating_sub(y)
                            .max(1)
                            .min(self.height.saturating_sub(y));
                        blit.set_scissor_rect(x, y, w, h);
                    }
                    blit.draw(0..6, 0..1);
                }
            }
        }

        if self.msaa_samples > 1 {
            // Resolve MSAA into retained_view so the idle-blit path has valid content next frame.
            {
                let _final = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("rsx-final-resolve"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &msaa_view,
                        resolve_target: Some(retained_view),
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Discard,
                        },
                    })],
                    depth_stencil_attachment: None,
                    occlusion_query_set: None,
                    timestamp_writes: None,
                    multiview_mask: None,
                });
            }
            // Blit retained to surface.
            let retained_bg = self.retained_blit_pipeline.create_bind_group(
                &self.device,
                retained_view,
                [
                    0.0,
                    0.0,
                    self.width as f32 / self.scale_factor,
                    self.height as f32 / self.scale_factor,
                ],
                1.0,
                0.0,
                [1.0, 1.0],
            );
            {
                let mut blit = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("rsx-retained-blit"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &surface_view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    occlusion_query_set: None,
                    timestamp_writes: None,
                    multiview_mask: None,
                });
                blit.set_pipeline(&self.retained_blit_pipeline.pipeline);
                blit.set_bind_group(0, &self.viewport_bind_group, &[]);
                blit.set_bind_group(1, &retained_bg, &[]);
                blit.draw(0..6, 0..1);
            }
        } else {
            // Android (msaa_samples==1): copy directly to surface to avoid alpha-compositing artifacts on Adreno drivers.
            let msaa_tex = self
                .msaa_texture
                .as_ref()
                .ok_or_else(|| RendererError::Backend("msaa_texture missing for copy".into()))?;
            encoder.copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: msaa_tex,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &output.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width: self.width,
                    height: self.height,
                    depth_or_array_layers: 1,
                },
            );
            // Copy msaa_texture → retained_texture so the idle-blit path has valid content for the next idle frame.
            if let Some(retained) = self.retained_texture.as_ref() {
                encoder.copy_texture_to_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: msaa_tex,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::TexelCopyTextureInfo {
                        texture: retained,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::Extent3d {
                        width: self.width,
                        height: self.height,
                        depth_or_array_layers: 1,
                    },
                );
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        // Safe to recycle now: the encoder is submitted, so no in-flight pass within this frame can alias these textures.
        for entry in frame_scratch_textures.drain(..) {
            return_pooled_texture(&mut self.texture_pool, entry);
        }

        // Rasterize accumulated GPU paths (Vello) and composite over the surface. Done after the main encoder submit so Vello's internal compute submission is ordered after the frame's base content; the composite then blends the path layer on top.
        #[cfg(feature = "vello-paths")]
        self.composite_vello_paths(&surface_view);

        tracing::debug!("hw render_frame: presenting {}x{}", self.width, self.height);
        output.present();
        let current_hash = hash_draw_commands(orig_commands);
        if current_hash != self.prev_commands_hash {
            self.prev_commands = orig_commands.to_vec();
            self.prev_commands_hash = current_hash;
        }
        self.prev_generation = self.incoming_generation;
        self.clear_pending();
        Ok(())
    }
}
