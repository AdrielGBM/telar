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
use crate::primitives::path::{PathFillData, PathPipeline, PathTessCache, PathVertex};
use crate::primitives::rect::{RectInstance, RectPipeline};
use crate::primitives::text::{TextInstance, TextPipeline};
use crate::primitives::{Viewport, create_viewport_bind_group_layout};

mod frame;
mod pool;
mod shadow;
mod steps;

use pool::{PooledTexture, create_viewport_pool_slot, preferred_format};
use shadow::{ShadowCacheKey, ShadowOp};
use steps::{DrawStep, flush_batch, flush_image_batch};

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
    text_shaper: renderer_text::TextShaper,
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
    path_tess_cache: PathTessCache,
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
    shadow_resolved_cache: HashMap<ShadowCacheKey, (wgpu::Texture, wgpu::TextureView)>,
    // Retained frame-wide shadow instance buffer + bind group, keyed by a hash of all pending shadow instances. Reused across frames so unchanged shadows skip per-frame create_buffer_init + create_bind_group.
    shadow_instances_cache: Option<(u64, wgpu::Buffer, wgpu::BindGroup)>,
    // LRU eviction order for shadow_resolved_cache: front is least-recently-used, back is most-recently-used.
    shadow_resolved_cache_order: VecDeque<ShadowCacheKey>,
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

// Safety: cross-thread transfer via JoinHandle happens before any DrawCommands are processed, so no Rc<> values exist at transfer time (prev_commands starts empty); after joining the renderer lives exclusively on the main thread.
unsafe impl<W: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static> Send
    for HardwareRenderer<W>
{
}

impl<W: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static> Drop for HardwareRenderer<W> {
    fn drop(&mut self) {
        // Release cached layer textures before the device so the driver can free their GPU memory.
        self.layer_resolved_cache.clear();
        // Block on pending GPU work before the device is destroyed; otherwise wgpu defers cleanup to a later maintenance cycle, keeping GPU memory allocated beyond this drop.
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
    }
}

// Hardware scroll-blit-with-clear: seed the offscreen with the previous frame shifted by the scroll delta so a cleared scrolling frame only redraws the exposed band. On by default for the MSAA (desktop) path; set RSX_HW_SCROLL_BLIT=0 to fall back to a full re-render.
fn hw_scroll_blit_enabled() -> bool {
    use std::sync::OnceLock;
    static V: OnceLock<bool> = OnceLock::new();
    *V.get_or_init(|| std::env::var("RSX_HW_SCROLL_BLIT").as_deref() != Ok("0"))
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
        let viewport_bind_group_layout = create_viewport_bind_group_layout(&device);
        let viewport_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rsx-viewport-bg"),
            layout: &viewport_bind_group_layout,
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
            &viewport_bind_group_layout,
            pipeline_cache.as_ref(),
            msaa_samples,
            config.image_gpu_max_age_frames,
        );
        let path_tess_cache = PathTessCache::new(config.path_tess_max_age_frames);

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

        let mut text_shaper = renderer_text::TextShaper::with_config(font_config);
        let font_metrics = text_shaper.font_metrics();

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
            text_shaper,
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
            path_tess_cache,
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
            layer_resolved_cache: HashMap::new(),
            layer_resolved_cache_order: VecDeque::new(),
            layer_cache_pixel_budget: 0,
            retained_texture: None,
            retained_view: None,
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
        self.surface = new_surface;
        self._window = window;
        // Force reconfiguration on the next begin_frame (begin_frame handles config.is_none()).
        self.config = None;
        self.viewport_dirty = true;
        Ok(())
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

        self.surface.configure(&self.device, &config);
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
