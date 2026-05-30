use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

use geometry_core::Rect;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use renderer_core::{Color, DrawCommand, ImageFilter, RenderBackend, RendererError};

use wgpu::util::DeviceExt;
use wgpu::{Device, Queue, Surface, SurfaceConfiguration, TextureViewDescriptor};

use crate::blur::BlurPipeline;
use crate::composite::CompositePipeline;
use crate::primitives::image::{ImageInstance, ImagePipeline};
use crate::primitives::layer::LayerPipeline;
use crate::primitives::line::{LineInstance, LinePipeline};
use crate::primitives::path::{PathFillData, PathPipeline, PathTessCache, PathVertex};
use crate::primitives::rect::{RectInstance, RectPipeline};
use crate::primitives::text::{TextInstance, TextPipeline};
use crate::primitives::{MSAA_SAMPLES, Viewport, create_viewport_bgl};

fn preferred_format(caps: &wgpu::SurfaceCapabilities) -> wgpu::TextureFormat {
    // Prefer Rgba8Unorm first: wider MSAA support on Linux/Vulkan; some Mesa drivers error on Bgra8Unorm MSAA textures. Fall back to Bgra8Unorm, then driver default.
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
    clip_radius: f32,
    begin_step_idx: usize,
    bounds: Option<Rect>,
}

#[inline]
fn union_rects(a: Rect, b: Rect) -> Rect {
    let x = a.x.min(b.x);
    let y = a.y.min(b.y);
    let right = (a.x + a.width).max(b.x + b.width);
    let bottom = (a.y + a.height).max(b.y + b.height);
    Rect::new(x, y, right - x, bottom - y)
}

#[inline]
fn hash_instances<T: bytemuck::Pod>(data: &[T]) -> u64 {
    let bytes: &[u8] = bytemuck::cast_slice(data);
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
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

#[derive(Hash, PartialEq, Eq)]
struct ShadowCacheKey {
    instance_start: u32,
    instance_count: u32,
    sigma_bits: u32,
    tex_w: u32,
    tex_h: u32,
    instances_hash: u64,
}

#[derive(Hash, PartialEq, Eq)]
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
    surface: Surface<'static>,
    device: Device,
    queue: Queue,
    config: Option<SurfaceConfiguration>,
    viewport_buffer: wgpu::Buffer,
    viewport_bind_group: wgpu::BindGroup,
    viewport_dirty: bool,
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
    path_shadow_resolved_cache: HashMap<PathShadowCacheKey, (wgpu::Texture, wgpu::TextureView)>,
    // Non-MSAA presentation texture holding the last resolved frame. Used both as the idle-frame fast-path source (blit when commands are unchanged) and as the MSAA resolve target each active frame.
    retained_texture: Option<wgpu::Texture>,
    retained_view: Option<wgpu::TextureView>,
    prev_commands: Vec<DrawCommand>,
    retained_blit_pipeline: crate::composite::CompositePipeline,
    prev_rect_hash: u64,
    prev_text_hash: u64,
    prev_line_hash: u64,
    prev_image_hash: u64,
    _window: std::sync::Arc<W>,
}

impl<W: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static> HardwareRenderer<W> {
    pub fn new(window: W, cache_path: Option<&std::path::Path>) -> Result<Self, RendererError> {
        pollster::block_on(Self::new_async(window, cache_path))
    }

    pub async fn new_async(
        window: W,
        cache_path: Option<&std::path::Path>,
    ) -> Result<Self, RendererError> {
        let window = std::sync::Arc::new(window);

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
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

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("rsx-hardware-renderer"),
                required_features: pipeline_cache_feature,
                required_limits: wgpu::Limits::default(),
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
        let present_mode = surface_caps
            .present_modes
            .iter()
            .find(|&&m| m == wgpu::PresentMode::Mailbox)
            .copied()
            .unwrap_or(wgpu::PresentMode::Fifo);
        let alpha_mode = surface_caps
            .alpha_modes
            .first()
            .copied()
            .unwrap_or(wgpu::CompositeAlphaMode::Auto);

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
            let t_rect = s.spawn(|| RectPipeline::new(&device, surface_format, &viewport_bgl, pc));
            let t_text = s.spawn(|| TextPipeline::new(&device, surface_format, &viewport_bgl, pc));
            let t_line = s.spawn(|| LinePipeline::new(&device, surface_format, &viewport_bgl, pc));
            let t_path = s.spawn(|| PathPipeline::new(&device, surface_format, &viewport_bgl, pc));
            let t_layer = s.spawn(|| LayerPipeline::new(&device, surface_format));
            let t_blur = s.spawn(|| BlurPipeline::new(&device, surface_format, pc));
            let t_composite = s.spawn(|| {
                CompositePipeline::new(&device, surface_format, MSAA_SAMPLES, &viewport_bgl, pc)
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
        );
        let path_tess_cache = PathTessCache::new();

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

        Ok(Self {
            surface,
            device,
            queue,
            config: None,
            viewport_buffer,
            viewport_bind_group,
            viewport_dirty: true,
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
            text_shaper: renderer_text::TextShaper::new(),
            surface_format,
            present_mode,
            alpha_mode,
            width: 0,
            height: 0,
            pending_instances: Vec::new(),
            pending_text_instances: Vec::new(),
            pending_line_instances: Vec::new(),
            pending_image_instances: Vec::new(),
            pending_steps: Vec::new(),
            pending_path_vertices: Vec::new(),
            pending_path_indices: Vec::new(),
            pending_path_fill_data: Vec::new(),
            path_tess_cache,
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
            path_shadow_resolved_cache: HashMap::new(),
            retained_texture: None,
            retained_view: None,
            prev_commands: Vec::new(),
            retained_blit_pipeline,
            prev_rect_hash: 0,
            prev_text_hash: 0,
            prev_line_hash: 0,
            prev_image_hash: 0,
            _window: window,
        })
    }

    fn reconfigure(&mut self, width: u32, height: u32) {
        let config = SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
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
        self.msaa_texture = Some(self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rsx-msaa"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: MSAA_SAMPLES,
            dimension: wgpu::TextureDimension::D2,
            format: self.surface_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        }));
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
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
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
}

fn fill_layer_alpha(style: &renderer_core::RectStyle) -> Option<f32> {
    if style.radius.is_zero() || style.shadow.is_some() {
        return None;
    }
    match style.fill {
        Some(renderer_core::FillStyle::Solid(c)) if c.a > 0.0 && c.a < 1.0 => Some(c.a),
        _ => None,
    }
}

fn expand_fill_layers(commands: &[DrawCommand]) -> Option<Vec<DrawCommand>> {
    if !commands
        .iter()
        .any(|cmd| matches!(cmd, DrawCommand::Rect(p) if fill_layer_alpha(&p.style).is_some()))
    {
        return None;
    }
    let mut result = Vec::with_capacity(commands.len() + 4);
    for cmd in commands {
        if let DrawCommand::Rect(p) = cmd {
            if let Some(alpha) = fill_layer_alpha(&p.style) {
                let mut opaque = (**p).clone();
                if let Some(renderer_core::FillStyle::Solid(c)) = opaque.style.fill {
                    opaque.style.fill =
                        Some(renderer_core::FillStyle::Solid(renderer_core::Color {
                            a: 1.0,
                            ..c
                        }));
                }
                result.push(DrawCommand::PushLayer {
                    opacity: alpha,
                    backdrop_blur: 0.0,
                    clip_radius: 0.0,
                });
                result.push(DrawCommand::Rect(Box::new(opaque)));
                result.push(DrawCommand::PopLayer);
                continue;
            }
        }
        result.push(cmd.clone());
    }
    Some(result)
}

impl<W: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static> RenderBackend
    for HardwareRenderer<W>
{
    fn begin_frame(&mut self, width: u32, height: u32) -> Result<(), RendererError> {
        if width != self.width || height != self.height || self.config.is_none() {
            // Pooled layer textures are sized to the previous surface dimensions and would be unusable at the new size; drop them so we don't leak GPU memory for textures we will never reuse.
            self.layer_texture_pool.clear();
            self.width = width;
            self.height = height;
            if width > 0 && height > 0 {
                self.reconfigure(width, height);
            }
        }
        self.clear_pending();
        self.path_tess_cache.begin_frame();
        self.image_pipeline.begin_frame();
        Ok(())
    }

    fn render_frame(
        &mut self,
        commands: &[DrawCommand],
        clear_color: Option<Color>,
    ) -> Result<(), RendererError> {
        // Idle-frame fast path: skip full pipeline and blit retained texture when commands and viewport are unchanged.
        if !self.prev_commands.is_empty()
            && commands == self.prev_commands.as_slice()
            && !self.viewport_dirty
            && self.config.is_some()
            && self.width > 0
            && self.height > 0
        {
            if let Some(retained_view) = self.retained_view.as_ref() {
                let output = match self.surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(t)
                    | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
                    wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                        if let Some(config) = &self.config.clone() {
                            self.surface.configure(&self.device, config);
                        }
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
                    .create_view(&TextureViewDescriptor::default());
                let retained_bg = self.retained_blit_pipeline.create_bind_group(
                    &self.device,
                    retained_view,
                    [0.0, 0.0, self.width as f32, self.height as f32],
                    1.0,
                    0.0,
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
                output.present();
                self.clear_pending();
                return Ok(());
            }
        }

        self.draw_state.reset();
        let scroll_blit = renderer_core::dirty::detect_scroll_blit(commands, &self.prev_commands);
        let dirty_scissor: Option<Rect> =
            if clear_color.is_none() && scroll_blit.is_none() && !self.prev_commands.is_empty() {
                renderer_core::dirty::compute_dirty_rect(
                    commands,
                    &self.prev_commands,
                    renderer_core::culling::command_visual_rect,
                )
            } else {
                None
            };
        let mut current_scissor: Option<Rect> = None;
        let mut scissor_layer_stack: Vec<Option<Rect>> = Vec::new(); // saves/restores current_scissor across PushLayer/PopLayer; layers disable frustum culling inside their bounds
        let mut layer_accum_stack: Vec<LayerAccum> = Vec::new();
        let layer_blit_stack: Vec<wgpu::BindGroup> = Vec::new();

        let orig_commands = commands;
        let expanded_commands = expand_fill_layers(commands);
        let commands: &[DrawCommand] = expanded_commands.as_deref().unwrap_or(commands);

        for cmd in commands {
            match cmd {
                DrawCommand::Rect(p) => {
                    if p.rect.width <= 0.0
                        || p.rect.height <= 0.0
                        || (p.style.fill.is_none() && p.style.stroke.is_none())
                    {
                        continue;
                    }
                    if let Some(bounds) = renderer_core::culling::command_visual_rect(
                        cmd,
                        self.draw_state.cum_tx,
                        self.draw_state.cum_ty,
                    ) {
                        if !renderer_core::culling::overlaps(
                            bounds.x,
                            bounds.y,
                            bounds.width,
                            bounds.height,
                            current_scissor,
                        ) {
                            continue;
                        }
                        if let Some(ds) = dirty_scissor {
                            if !renderer_core::culling::overlaps(
                                bounds.x,
                                bounds.y,
                                bounds.width,
                                bounds.height,
                                Some(ds),
                            ) {
                                continue;
                            }
                        }
                        if let Some(ref sb) = scroll_blit {
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
                                continue;
                            }
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
                    let translated = Rect::new(
                        p.rect.x + self.draw_state.cum_tx,
                        p.rect.y + self.draw_state.cum_ty,
                        p.rect.width,
                        p.rect.height,
                    );
                    let inst = crate::primitives::rect::prepare_rect(
                        translated,
                        &p.style,
                        self.draw_state.cum_tx,
                        self.draw_state.cum_ty,
                    );
                    self.pending_instances.push(inst);
                }
                DrawCommand::Text(p) => {
                    if let Some(bounds) = renderer_core::culling::command_visual_rect(
                        cmd,
                        self.draw_state.cum_tx,
                        self.draw_state.cum_ty,
                    ) {
                        if !renderer_core::culling::overlaps(
                            bounds.x,
                            bounds.y,
                            bounds.width,
                            bounds.height,
                            current_scissor,
                        ) {
                            continue;
                        }
                        if let Some(ds) = dirty_scissor {
                            if !renderer_core::culling::overlaps(
                                bounds.x,
                                bounds.y,
                                bounds.width,
                                bounds.height,
                                Some(ds),
                            ) {
                                continue;
                            }
                        }
                        if let Some(ref sb) = scroll_blit {
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
                                continue;
                            }
                        }
                        if let Some(accum) = layer_accum_stack.last_mut() {
                            accum.bounds =
                                Some(accum.bounds.map_or(bounds, |b| union_rects(b, bounds)));
                        }
                    }
                    self.flush_rect();
                    self.flush_line();
                    self.flush_image();
                    let translated = Rect::new(
                        p.rect.x + self.draw_state.cum_tx,
                        p.rect.y + self.draw_state.cum_ty,
                        p.rect.width,
                        p.rect.height,
                    );
                    if let Some(shadow) = p.style.shadow {
                        self.flush_text();

                        let s = shadow.blur_radius / 2.0;
                        let box_r = (s * 1.5).round().max(1.0);
                        let sigma = (box_r * (box_r + 1.0)).sqrt();
                        let padding = (sigma * 3.0).ceil() as u32 + 2;
                        let shadow_rect = Rect::new(
                            translated.x + shadow.offset_x,
                            translated.y + shadow.offset_y,
                            translated.width,
                            translated.height,
                        );
                        let origin_x = shadow_rect.x - padding as f32;
                        let origin_y = shadow_rect.y - padding as f32;
                        let tex_w = (shadow_rect.width.ceil() as u32 + 2 * padding).max(1);
                        let tex_h = (shadow_rect.height.ceil() as u32 + 2 * padding).max(1);

                        let shadow_style = renderer_core::TextStyle {
                            color: shadow.color,
                            shadow: None,
                            ..p.style
                        };
                        let instance_start = self.pending_shadow_instances.len() as u32;
                        crate::primitives::text::prepare_text(
                            &mut self.text_shaper,
                            &p.text,
                            shadow_rect,
                            &shadow_style,
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
                            sigma,
                            tex_w,
                            tex_h,
                            dest: [origin_x, origin_y, tex_w as f32, tex_h as f32],
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
                        &p.text,
                        translated,
                        &p.style,
                        &mut self.pending_text_instances,
                    );
                }
                DrawCommand::Line { p1, p2, style } => {
                    if let Some(bounds) = renderer_core::culling::command_visual_rect(
                        cmd,
                        self.draw_state.cum_tx,
                        self.draw_state.cum_ty,
                    ) {
                        if !renderer_core::culling::overlaps(
                            bounds.x,
                            bounds.y,
                            bounds.width,
                            bounds.height,
                            current_scissor,
                        ) {
                            continue;
                        }
                        if let Some(ds) = dirty_scissor {
                            if !renderer_core::culling::overlaps(
                                bounds.x,
                                bounds.y,
                                bounds.width,
                                bounds.height,
                                Some(ds),
                            ) {
                                continue;
                            }
                        }
                        if let Some(ref sb) = scroll_blit {
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
                                continue;
                            }
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
                    let tp1 =
                        Point::new(p1.x + self.draw_state.cum_tx, p1.y + self.draw_state.cum_ty);
                    let tp2 =
                        Point::new(p2.x + self.draw_state.cum_tx, p2.y + self.draw_state.cum_ty);
                    self.pending_line_instances
                        .push(crate::primitives::line::prepare_line(tp1, tp2, *style));
                }
                DrawCommand::Image { data, rect, filter } => {
                    if let Some(bounds) = renderer_core::culling::command_visual_rect(
                        cmd,
                        self.draw_state.cum_tx,
                        self.draw_state.cum_ty,
                    ) {
                        if !renderer_core::culling::overlaps(
                            bounds.x,
                            bounds.y,
                            bounds.width,
                            bounds.height,
                            current_scissor,
                        ) {
                            continue;
                        }
                        if let Some(ds) = dirty_scissor {
                            if !renderer_core::culling::overlaps(
                                bounds.x,
                                bounds.y,
                                bounds.width,
                                bounds.height,
                                Some(ds),
                            ) {
                                continue;
                            }
                        }
                        if let Some(ref sb) = scroll_blit {
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
                                continue;
                            }
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
                    let translated = Rect::new(
                        rect.x + self.draw_state.cum_tx,
                        rect.y + self.draw_state.cum_ty,
                        rect.width,
                        rect.height,
                    );
                    self.pending_image_instances
                        .push(crate::primitives::image::prepare_image(translated));
                }
                DrawCommand::Path(p) => {
                    if let Some(bounds) = renderer_core::culling::command_visual_rect(
                        cmd,
                        self.draw_state.cum_tx,
                        self.draw_state.cum_ty,
                    ) {
                        if !renderer_core::culling::overlaps(
                            bounds.x,
                            bounds.y,
                            bounds.width,
                            bounds.height,
                            current_scissor,
                        ) {
                            continue;
                        }
                        if let Some(ref sb) = scroll_blit {
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
                                continue;
                            }
                        }
                        if let Some(accum) = layer_accum_stack.last_mut() {
                            accum.bounds =
                                Some(accum.bounds.map_or(bounds, |b| union_rects(b, bounds)));
                        }
                    }
                    self.flush_all();

                    if let Some(shadow) = p.style.shadow {
                        let shadow_fill = p
                            .style
                            .fill
                            .map(|_| renderer_core::FillStyle::Solid(shadow.color));
                        let shadow_stroke = p.style.stroke.map(|s| renderer_core::Stroke {
                            color: shadow.color,
                            ..s
                        });
                        let shadow_style = renderer_core::PathStyle {
                            fill: shadow_fill,
                            stroke: shadow_stroke,
                            fill_rule: p.style.fill_rule,
                            shadow: None,
                        };

                        let sv_start = self.pending_shadow_path_vertices.len();
                        let si_start = self.pending_shadow_path_indices.len() as u32;
                        crate::primitives::path::prepare_path(
                            &mut self.path_tess_cache,
                            &p.data,
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

                            let world_min_x = min_x + self.draw_state.cum_tx + shadow.offset_x;
                            let world_min_y = min_y + self.draw_state.cum_ty + shadow.offset_y;
                            let world_max_x = max_x + self.draw_state.cum_tx + shadow.offset_x;
                            let world_max_y = max_y + self.draw_state.cum_ty + shadow.offset_y;

                            let s = shadow.blur_radius / 2.0;
                            let box_r = (s * 1.5).round().max(1.0);
                            let sigma = (box_r * (box_r + 1.0)).sqrt();
                            let padding = (sigma * 3.0).ceil() as u32 + 2;

                            let origin_x = world_min_x - padding as f32;
                            let origin_y = world_min_y - padding as f32;
                            let tex_w =
                                ((world_max_x - world_min_x).ceil() as u32 + 2 * padding).max(1);
                            let tex_h =
                                ((world_max_y - world_min_y).ceil() as u32 + 2 * padding).max(1);

                            for v in &mut self.pending_shadow_path_vertices[sv_start..] {
                                v.position[0] +=
                                    self.draw_state.cum_tx + shadow.offset_x - origin_x;
                                v.position[1] +=
                                    self.draw_state.cum_ty + shadow.offset_y - origin_y;
                            }

                            self.pending_path_shadow_ops.push(PathShadowOp {
                                index_start: si_start,
                                index_end: si_end,
                                sigma,
                                tex_w,
                                tex_h,
                                dest: [origin_x, origin_y, tex_w as f32, tex_h as f32],
                            });
                            self.pending_steps.push(DrawStep::PathShadowPlaceholder {
                                op_idx: self.pending_path_shadow_ops.len() - 1,
                            });
                        }
                    }

                    let vertex_start = self.pending_path_vertices.len();
                    let index_start = self.pending_path_indices.len() as u32;
                    let fill_data_start = self.pending_path_fill_data.len();
                    crate::primitives::path::prepare_path(
                        &mut self.path_tess_cache,
                        &p.data,
                        &p.style,
                        &mut self.pending_path_vertices,
                        &mut self.pending_path_indices,
                        &mut self.pending_path_fill_data,
                    );
                    for v in &mut self.pending_path_vertices[vertex_start..] {
                        v.position[0] += self.draw_state.cum_tx;
                        v.position[1] += self.draw_state.cum_ty;
                    }
                    for fd in &mut self.pending_path_fill_data[fill_data_start..] {
                        fd.grad_p0[0] += self.draw_state.cum_tx;
                        fd.grad_p0[1] += self.draw_state.cum_ty;
                        fd.grad_p1[0] += self.draw_state.cum_tx;
                        fd.grad_p1[1] += self.draw_state.cum_ty;
                    }
                    let index_end = self.pending_path_indices.len() as u32;
                    if index_end > index_start {
                        self.pending_steps.push(DrawStep::PathDraw {
                            index_start,
                            index_end,
                        });
                    }
                }
                DrawCommand::PushClip { rect } => {
                    self.flush_all();
                    let effective = self.draw_state.push_clip(*rect);
                    current_scissor = Some(effective);
                    self.pending_steps.push(DrawStep::SetScissor {
                        rect: Some(effective),
                    });
                }
                DrawCommand::PopClip => {
                    self.flush_all();
                    let effective = self.draw_state.pop_clip();
                    current_scissor = effective;
                    self.pending_steps
                        .push(DrawStep::SetScissor { rect: effective });
                }
                DrawCommand::PushTransform { tx, ty } => {
                    self.draw_state.push_transform(*tx, *ty);
                }
                DrawCommand::PopTransform => {
                    self.draw_state.pop_transform();
                }
                DrawCommand::PushLayer {
                    opacity,
                    backdrop_blur,
                    clip_radius,
                } => {
                    self.flush_all();
                    // Disable frustum culling inside the layer to avoid incorrect culling by an outer PushClip; save scissor for restore at PopLayer.
                    scissor_layer_stack.push(current_scissor);
                    current_scissor = None;
                    layer_accum_stack.push(LayerAccum {
                        opacity: *opacity,
                        backdrop_blur: *backdrop_blur,
                        clip_radius: *clip_radius,
                        begin_step_idx: self.pending_steps.len(),
                        bounds: None,
                    });
                }
                DrawCommand::PopLayer => {
                    self.flush_all();
                    current_scissor = scissor_layer_stack.pop().flatten();
                    if let Some(accum) = layer_accum_stack.pop() {
                        let (offset_x, offset_y, tex_w, tex_h) = if let Some(b) = accum.bounds {
                            let ox = b.x.floor().max(0.0);
                            let oy = b.y.floor().max(0.0);
                            let w = (b.width.ceil() as u32).max(1).min(self.width.max(1));
                            let h = (b.height.ceil() as u32).max(1).min(self.height.max(1));
                            (ox, oy, w, h)
                        } else {
                            (0.0, 0.0, self.width.max(1), self.height.max(1))
                        };
                        let (msaa_texture, msaa_view, resolve_texture, resolve_view) =
                            if let Some(pos) = self
                                .layer_texture_pool
                                .iter()
                                .position(|(_, _, _, _, pw, ph)| *pw == tex_w && *ph == tex_h)
                            {
                                let (mt, mv, rt, rv, _, _) = self.layer_texture_pool.remove(pos);
                                (mt, mv, rt, rv)
                            } else {
                                self.layer_pipeline.create_layer_textures(
                                    &self.device,
                                    tex_w,
                                    tex_h,
                                )
                            };
                        let layer_vp = Viewport {
                            size: [tex_w as f32, tex_h as f32],
                            offset: [offset_x, offset_y],
                        };
                        let layer_vp_buf =
                            self.device
                                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                    label: Some("rsx-layer-vp"),
                                    contents: bytemuck::bytes_of(&layer_vp),
                                    usage: wgpu::BufferUsages::UNIFORM,
                                });
                        let layer_vp_bg =
                            self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                                label: Some("rsx-layer-vp-bg"),
                                layout: &self.viewport_bgl,
                                entries: &[wgpu::BindGroupEntry {
                                    binding: 0,
                                    resource: layer_vp_buf.as_entire_binding(),
                                }],
                            });
                        // Composite bind group uses window-absolute dest rect; parent viewport (set 0) converts it to NDC.
                        let composite_bg = self.composite_pipeline.create_bind_group(
                            &self.device,
                            &resolve_view,
                            [offset_x, offset_y, tex_w as f32, tex_h as f32],
                            accum.opacity,
                            accum.clip_radius,
                        );
                        self.pending_steps.insert(
                            accum.begin_step_idx,
                            DrawStep::BeginLayer {
                                msaa_texture,
                                msaa_view,
                                resolve_texture,
                                resolve_view,
                                viewport_bind_group: layer_vp_bg,
                                width: tex_w,
                                height: tex_h,
                                offset_x,
                                offset_y,
                                backdrop_blur: accum.backdrop_blur,
                            },
                        );
                        self.pending_steps.push(DrawStep::EndLayerComposite {
                            bind_group: composite_bg,
                        });
                    }
                    let _ = layer_blit_stack; // suppresses unused variable warning
                }
            }
        }

        self.flush_all();

        let load_op = if let Some(c) = clear_color {
            wgpu::LoadOp::Clear(wgpu::Color {
                r: c.r as f64,
                g: c.g as f64,
                b: c.b as f64,
                a: c.a as f64,
            })
        } else {
            wgpu::LoadOp::Load
        };

        if self.config.is_none() || self.width == 0 || self.height == 0 {
            self.clear_pending();
            return Ok(());
        }

        let output = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                if let Some(config) = &self.config.clone() {
                    self.surface.configure(&self.device, config);
                }
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

        let (shadow_results, path_shadow_results): (
            Vec<Option<wgpu::BindGroup>>,
            Vec<Option<wgpu::BindGroup>>,
        ) = if has_text_shadows || has_path_shadows {
            let shadow_buf_opt = if has_text_shadows {
                Some(
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("rsx-shadow-instances"),
                            contents: bytemuck::cast_slice(&self.pending_shadow_instances),
                            usage: wgpu::BufferUsages::STORAGE,
                        }),
                )
            } else {
                None
            };
            let shadow_instances_bg_opt = shadow_buf_opt.as_ref().map(|buf| {
                self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("rsx-shadow-instances-bg"),
                    layout: &self.text_pipeline.instances.instances_bgl,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: buf.as_entire_binding(),
                    }],
                })
            });

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

            let mut pre_encoder =
                self.device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("rsx-shadow-pre-encoder"),
                    });

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
                        let bg = self.composite_pipeline.create_bind_group(
                            &self.device,
                            cached_view,
                            op.dest,
                            1.0,
                            0.0,
                        );
                        text_results.push(Some(bg));
                        continue;
                    }

                    let (cap_msaa_texture, cap_msaa_view, cap_resolve_texture, cap_resolve_view) =
                        if let Some(pos) = self
                            .shadow_capture_pool
                            .iter()
                            .position(|(_, _, _, _, w, h)| *w == op.tex_w && *h == op.tex_h)
                        {
                            let (mt, mv, rt, rv, _, _) = self.shadow_capture_pool.remove(pos);
                            (mt, mv, rt, rv)
                        } else {
                            self.layer_pipeline.create_layer_textures(
                                &self.device,
                                op.tex_w,
                                op.tex_h,
                            )
                        };

                    let vp_data: [f32; 4] = [op.tex_w as f32, op.tex_h as f32, 0.0, 0.0];
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
                        let mut pass = pre_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("rsx-shadow-capture"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: &cap_msaa_view,
                                resolve_target: Some(&cap_resolve_view),
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
                        pass.set_pipeline(&self.text_pipeline.pipeline);
                        pass.set_bind_group(0, &shadow_vp_bg, &[]);
                        pass.set_bind_group(1, &shadow_instances_bg, &[]);
                        pass.set_bind_group(2, &self.text_pipeline.atlas_bind_group, &[]);
                        pass.draw(0..6, op.instance_start..op.instance_end);
                    }

                    let (blurred_texture, blurred_view) = self.blur_pipeline.apply(
                        &self.device,
                        &mut pre_encoder,
                        &cap_resolve_view,
                        op.tex_w,
                        op.tex_h,
                        op.sigma,
                    );
                    let bg = self.composite_pipeline.create_bind_group(
                        &self.device,
                        &blurred_view,
                        op.dest,
                        1.0,
                        0.0,
                    );
                    text_results.push(Some(bg));
                    if self.shadow_resolved_cache.len() >= 128 {
                        self.shadow_resolved_cache.clear();
                    }
                    self.shadow_resolved_cache
                        .insert(key, (blurred_texture, blurred_view));
                    self.shadow_capture_pool.push((
                        cap_msaa_texture,
                        cap_msaa_view,
                        cap_resolve_texture,
                        cap_resolve_view,
                        op.tex_w,
                        op.tex_h,
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
                        let mut hasher = DefaultHasher::new();
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
                        let bg = self.composite_pipeline.create_bind_group(
                            &self.device,
                            cached_view,
                            op.dest,
                            1.0,
                            0.0,
                        );
                        path_results.push(Some(bg));
                        continue;
                    }

                    let (cap_msaa_texture, cap_msaa_view, cap_resolve_texture, cap_resolve_view) =
                        if let Some(pos) = self
                            .shadow_capture_pool
                            .iter()
                            .position(|(_, _, _, _, w, h)| *w == op.tex_w && *h == op.tex_h)
                        {
                            let (mt, mv, rt, rv, _, _) = self.shadow_capture_pool.remove(pos);
                            (mt, mv, rt, rv)
                        } else {
                            self.layer_pipeline.create_layer_textures(
                                &self.device,
                                op.tex_w,
                                op.tex_h,
                            )
                        };

                    let vp_data: [f32; 4] = [op.tex_w as f32, op.tex_h as f32, 0.0, 0.0];
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
                        let mut pass = pre_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("rsx-shadow-path-capture"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: &cap_msaa_view,
                                resolve_target: Some(&cap_resolve_view),
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
                        pass.set_pipeline(&self.path_pipeline.pipeline);
                        pass.set_bind_group(0, &shadow_vp_bg, &[]);
                        pass.set_bind_group(1, &shadow_path_fd_bg, &[]);
                        pass.set_vertex_buffer(0, shadow_path_vb.slice(..));
                        pass.set_index_buffer(shadow_path_ib.slice(..), wgpu::IndexFormat::Uint32);
                        pass.draw_indexed(op.index_start..op.index_end, 0, 0..1);
                    }

                    let (blurred_texture, blurred_view) = self.blur_pipeline.apply(
                        &self.device,
                        &mut pre_encoder,
                        &cap_resolve_view,
                        op.tex_w,
                        op.tex_h,
                        op.sigma,
                    );
                    let bg = self.composite_pipeline.create_bind_group(
                        &self.device,
                        &blurred_view,
                        op.dest,
                        1.0,
                        0.0,
                    );
                    path_results.push(Some(bg));
                    if self.path_shadow_resolved_cache.len() >= 128 {
                        self.path_shadow_resolved_cache.clear();
                    }
                    self.path_shadow_resolved_cache
                        .insert(path_key, (blurred_texture, blurred_view));
                    self.shadow_capture_pool.push((
                        cap_msaa_texture,
                        cap_msaa_view,
                        cap_resolve_texture,
                        cap_resolve_view,
                        op.tex_w,
                        op.tex_h,
                    ));
                }
            }

            self.queue.submit(std::iter::once(pre_encoder.finish()));
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

        let surface_view = output
            .texture
            .create_view(&TextureViewDescriptor::default());

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

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("rsx-encoder"),
            });

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
            EndLayerComposite(wgpu::BindGroup),
        }

        let mut steps = std::mem::take(&mut self.pending_steps);
        let mut segments: Vec<Segment> = Vec::new();
        // Walk steps emitting Segment::Draw with index ranges; extract layer-boundary steps in place via std::mem::replace to avoid moving ownership-bearing variants.
        let mut current_start: usize = 0;
        for i in 0..steps.len() {
            let is_boundary = matches!(
                steps[i],
                DrawStep::BeginLayer { .. } | DrawStep::EndLayerComposite { .. }
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
                DrawStep::EndLayerComposite { bind_group } => {
                    segments.push(Segment::EndLayerComposite(bind_group));
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

        // Marks draw segments preceding EndLayerComposite to inline MSAA resolve into the drawing pass, skipping the dedicated resolve pass.
        let mut inline_resolve_targets: Vec<bool> = vec![false; segments.len()];
        for i in 0..segments.len() {
            if let (Segment::Draw { .. }, Some(Segment::EndLayerComposite(_))) =
                (&segments[i], segments.get(i + 1))
            {
                inline_resolve_targets[i] = true;
            }
        }

        let mut endlayer_resolve_done: Vec<bool> = vec![false; segments.len()];
        for i in 0..segments.len() {
            if matches!(segments[i], Segment::EndLayerComposite(_))
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
                        if let Some((_, lv, _, _, _, _, _)) = layer_stack.last() {
                            lv
                        } else {
                            &msaa_view
                        };
                    let resolve_view_opt: Option<&wgpu::TextureView> = if inline_resolve {
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
                                store: wgpu::StoreOp::Store,
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
                                        let x = (r.x.max(0.0).floor() as u32)
                                            .min(self.width.saturating_sub(1));
                                        let y = (r.y.max(0.0).floor() as u32)
                                            .min(self.height.saturating_sub(1));
                                        let right = ((r.x + r.width).ceil() as u32).min(self.width);
                                        let bottom =
                                            ((r.y + r.height).ceil() as u32).min(self.height);
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
                            DrawStep::BeginLayer { .. } | DrawStep::EndLayerComposite { .. } => {
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
                        let _clear = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("rsx-layer-clear"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: &layer_msaa_view,
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
                        // Resolve parent MSAA into a temp single-sample texture so it can be sampled.
                        let (parent_w, parent_h) =
                            if let Some((_, _, _, _, _, pw, ph)) = layer_stack.last() {
                                (*pw, *ph)
                            } else {
                                (self.width, self.height)
                            };
                        let parent_msaa_view: &wgpu::TextureView =
                            if let Some((_, pmv, _, _, _, _, _)) = layer_stack.last() {
                                pmv
                            } else {
                                &msaa_view
                            };

                        let temp_resolve = self.device.create_texture(&wgpu::TextureDescriptor {
                            label: Some("rsx-backdrop-resolve"),
                            size: wgpu::Extent3d {
                                width: parent_w.max(1),
                                height: parent_h.max(1),
                                depth_or_array_layers: 1,
                            },
                            mip_level_count: 1,
                            sample_count: 1,
                            dimension: wgpu::TextureDimension::D2,
                            format: self.surface_format,
                            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                                | wgpu::TextureUsages::COPY_SRC,
                            view_formats: &[],
                        });
                        let temp_resolve_view =
                            temp_resolve.create_view(&wgpu::TextureViewDescriptor::default());

                        {
                            let _resolve = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                label: Some("rsx-backdrop-parent-resolve"),
                                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                    view: parent_msaa_view,
                                    resolve_target: Some(&temp_resolve_view),
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
                        }

                        let ox_px = offset_x.floor().max(0.0) as u32;
                        let oy_px = offset_y.floor().max(0.0) as u32;
                        let crop_w = width.min(parent_w.saturating_sub(ox_px));
                        let crop_h = height.min(parent_h.saturating_sub(oy_px));

                        let cropped = self.device.create_texture(&wgpu::TextureDescriptor {
                            label: Some("rsx-backdrop-crop"),
                            size: wgpu::Extent3d {
                                width: crop_w.max(1),
                                height: crop_h.max(1),
                                depth_or_array_layers: 1,
                            },
                            mip_level_count: 1,
                            sample_count: 1,
                            dimension: wgpu::TextureDimension::D2,
                            format: self.surface_format,
                            usage: wgpu::TextureUsages::COPY_DST
                                | wgpu::TextureUsages::TEXTURE_BINDING,
                            view_formats: &[],
                        });

                        if crop_w > 0 && crop_h > 0 {
                            encoder.copy_texture_to_texture(
                                wgpu::TexelCopyTextureInfo {
                                    texture: &temp_resolve,
                                    mip_level: 0,
                                    origin: wgpu::Origin3d {
                                        x: ox_px,
                                        y: oy_px,
                                        z: 0,
                                    },
                                    aspect: wgpu::TextureAspect::All,
                                },
                                wgpu::TexelCopyTextureInfo {
                                    texture: &cropped,
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

                        let cropped_view =
                            cropped.create_view(&wgpu::TextureViewDescriptor::default());
                        let (_blurred_tex, blurred_view) = self.blur_pipeline.apply(
                            &self.device,
                            &mut encoder,
                            &cropped_view,
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
                        );
                        {
                            let mut backdrop_pass =
                                encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                    label: Some("rsx-backdrop-composite"),
                                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                        view: &layer_msaa_view,
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

                Segment::EndLayerComposite(bind_group) => {
                    let (l_msaa_tex, l_msaa_view, l_resolve_tex, l_resolve_view, _, lw, lh) =
                        layer_stack
                            .pop()
                            .expect("layer_stack underflow on EndLayerComposite");

                    // If the preceding draw pass set its resolve_target to this layer's resolve view, the MSAA resolve already happened when that pass ended — skip the dedicated resolve pass.
                    if !endlayer_resolve_done[seg_idx] {
                        let _resolve = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("rsx-layer-resolve"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: &l_msaa_view,
                                resolve_target: Some(&l_resolve_view),
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
                    }

                    let parent_view: &wgpu::TextureView =
                        if let Some((_, pv, _, _, _, _, _)) = layer_stack.last() {
                            pv
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
                        blit.draw(0..6, 0..1);
                    }

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
        }

        {
            let _final = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rsx-final-resolve"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &msaa_view,
                    resolve_target: Some(retained_view),
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
        }

        {
            let retained_bg = self.retained_blit_pipeline.create_bind_group(
                &self.device,
                retained_view,
                [0.0, 0.0, self.width as f32, self.height as f32],
                1.0,
                0.0,
            );
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

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        self.prev_commands = orig_commands.to_vec();
        self.clear_pending();
        Ok(())
    }
}
