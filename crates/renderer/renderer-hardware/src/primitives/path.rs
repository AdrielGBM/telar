use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Arc;

use lyon::math::point;
use lyon::path::LineCap as LyonLineCap;
use lyon::path::LineJoin as LyonLineJoin;
use lyon::path::Path;
use lyon::tessellation::{
    BuffersBuilder, FillOptions, FillTessellator, FillVertex, StrokeOptions, StrokeTessellator,
    StrokeVertex, VertexBuffers,
};
use renderer_core::{FillRule, LineCap, LineJoin, PathData, PathStyle, PathVerb};
use wgpu::Device;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct PathVertex {
    pub position: [f32; 2],
    pub color: [f32; 4],
}

#[derive(Hash, PartialEq, Eq, Clone, Copy)]
struct FillGeomKey {
    ptr: usize,
    even_odd: bool,
}

#[derive(Hash, PartialEq, Eq, Clone, Copy)]
struct StrokeGeomKey {
    ptr: usize,
    width_bits: u32,
    cap: u8,
    join: u8,
}

const PATH_TESS_MAX_AGE_FRAMES: u64 = 120; // ~2 s at 60 fps

struct CachedGeom {
    positions: Vec<[f32; 2]>,
    indices: Vec<u32>,
    last_frame: u64,
}

pub(crate) struct PathTessCache {
    fill: HashMap<FillGeomKey, CachedGeom>,
    stroke: HashMap<StrokeGeomKey, CachedGeom>,
    fill_lru: VecDeque<(FillGeomKey, u64)>,
    stroke_lru: VecDeque<(StrokeGeomKey, u64)>,
    frame: u64,
}

impl PathTessCache {
    pub(crate) fn new() -> Self {
        Self {
            fill: HashMap::new(),
            stroke: HashMap::new(),
            fill_lru: VecDeque::new(),
            stroke_lru: VecDeque::new(),
            frame: 0,
        }
    }

    pub(crate) fn begin_frame(&mut self) {
        self.frame += 1;
        let current = self.frame;

        // queue is oldest-first; stop at first entry within threshold
        while let Some(&(key, queued_frame)) = self.fill_lru.front() {
            if current - queued_frame <= PATH_TESS_MAX_AGE_FRAMES {
                break;
            }
            self.fill_lru.pop_front();
            if let Some(entry) = self.fill.get(&key) {
                // stale if re-accessed since queued; a newer queue entry will handle eviction
                if entry.last_frame == queued_frame {
                    self.fill.remove(&key);
                }
            }
        }

        while let Some(&(key, queued_frame)) = self.stroke_lru.front() {
            if current - queued_frame <= PATH_TESS_MAX_AGE_FRAMES {
                break;
            }
            self.stroke_lru.pop_front();
            if let Some(entry) = self.stroke.get(&key) {
                if entry.last_frame == queued_frame {
                    self.stroke.remove(&key);
                }
            }
        }
    }
}

// returns true on first touch this frame; caller pushes a fresh lru entry only then
fn emit_cached_geom(
    geom: &mut CachedGeom,
    color: renderer_core::Color,
    current_frame: u64,
    out_vertices: &mut Vec<PathVertex>,
    out_indices: &mut Vec<u32>,
) -> bool {
    let touched = geom.last_frame != current_frame;
    if touched {
        geom.last_frame = current_frame;
    }
    let color_array = color.to_array();
    let vertex_base = out_vertices.len() as u32;
    for &pos in &geom.positions {
        out_vertices.push(PathVertex {
            position: pos,
            color: color_array,
        });
    }
    for &idx in &geom.indices {
        out_indices.push(vertex_base + idx);
    }
    touched
}

fn build_lyon_path(data: &PathData) -> Path {
    let mut builder = Path::builder();
    let mut in_path = false;

    for verb in &data.verbs {
        match verb {
            PathVerb::MoveTo(p) => {
                if in_path {
                    builder.end(false);
                }
                builder.begin(point(p.x, p.y));
                in_path = true;
            }
            PathVerb::LineTo(p) => {
                builder.line_to(point(p.x, p.y));
            }
            PathVerb::QuadTo { ctrl, to } => {
                builder.quadratic_bezier_to(point(ctrl.x, ctrl.y), point(to.x, to.y));
            }
            PathVerb::CubicTo { ctrl1, ctrl2, to } => {
                builder.cubic_bezier_to(
                    point(ctrl1.x, ctrl1.y),
                    point(ctrl2.x, ctrl2.y),
                    point(to.x, to.y),
                );
            }
            PathVerb::Close => {
                if in_path {
                    builder.end(true);
                    in_path = false;
                }
            }
        }
    }

    if in_path {
        builder.end(false);
    }

    builder.build()
}

pub(crate) fn prepare_path(
    cache: &mut PathTessCache,
    data: &Arc<PathData>,
    style: &PathStyle,
    out_vertices: &mut Vec<PathVertex>,
    out_indices: &mut Vec<u32>,
) {
    let current_frame = cache.frame;
    let ptr = Arc::as_ptr(data) as usize;

    let mut lyon_path: Option<Path> = None;

    if let Some(fill_style) = style.fill {
        let fill_key = FillGeomKey {
            ptr,
            even_odd: style.fill_rule == FillRule::EvenOdd,
        };

        if !cache.fill.contains_key(&fill_key) {
            let lyon_path = lyon_path.get_or_insert_with(|| build_lyon_path(data));
            let mut geometry: VertexBuffers<[f32; 2], u32> = VertexBuffers::new();
            let mut tessellator = FillTessellator::new();
            let lyon_fill_rule = if fill_key.even_odd {
                lyon::tessellation::FillRule::EvenOdd
            } else {
                lyon::tessellation::FillRule::NonZero
            };
            let options = FillOptions::default().with_fill_rule(lyon_fill_rule);
            if tessellator
                .tessellate_path(
                    &*lyon_path,
                    &options,
                    &mut BuffersBuilder::new(&mut geometry, |v: FillVertex| {
                        [v.position().x, v.position().y]
                    }),
                )
                .is_ok()
            {
                cache.fill.insert(
                    fill_key,
                    CachedGeom {
                        positions: geometry.vertices,
                        indices: geometry.indices,
                        last_frame: current_frame,
                    },
                );
                cache.fill_lru.push_back((fill_key, current_frame));
            }
        }

        if let Some(geom) = cache.fill.get_mut(&fill_key) {
            if emit_cached_geom(
                geom,
                fill_style.color(),
                current_frame,
                out_vertices,
                out_indices,
            ) {
                cache.fill_lru.push_back((fill_key, current_frame));
            }
        }
    }

    if let Some(s) = style.stroke {
        let stroke_key = StrokeGeomKey {
            ptr,
            width_bits: s.width.to_bits(),
            cap: s.cap as u8,
            join: s.join as u8,
        };

        if !cache.stroke.contains_key(&stroke_key) {
            let lyon_path = lyon_path.get_or_insert_with(|| build_lyon_path(data));
            let mut geometry: VertexBuffers<[f32; 2], u32> = VertexBuffers::new();
            let mut tessellator = StrokeTessellator::new();
            let line_cap = to_lyon_line_cap(s.cap);
            let line_join = to_lyon_line_join(s.join);
            let options = StrokeOptions::default()
                .with_line_width(s.width)
                .with_start_cap(line_cap)
                .with_end_cap(line_cap)
                .with_line_join(line_join);
            if tessellator
                .tessellate_path(
                    &*lyon_path,
                    &options,
                    &mut BuffersBuilder::new(&mut geometry, |v: StrokeVertex| {
                        [v.position().x, v.position().y]
                    }),
                )
                .is_ok()
            {
                cache.stroke.insert(
                    stroke_key,
                    CachedGeom {
                        positions: geometry.vertices,
                        indices: geometry.indices,
                        last_frame: current_frame,
                    },
                );
                cache.stroke_lru.push_back((stroke_key, current_frame));
            }
        }

        if let Some(geom) = cache.stroke.get_mut(&stroke_key) {
            if emit_cached_geom(geom, s.color, current_frame, out_vertices, out_indices) {
                cache.stroke_lru.push_back((stroke_key, current_frame));
            }
        }
    }
}

fn to_lyon_line_cap(cap: LineCap) -> LyonLineCap {
    match cap {
        LineCap::Butt => LyonLineCap::Butt,
        LineCap::Round => LyonLineCap::Round,
        LineCap::Square => LyonLineCap::Square,
    }
}

fn to_lyon_line_join(join: LineJoin) -> LyonLineJoin {
    match join {
        LineJoin::Miter => LyonLineJoin::Miter,
        LineJoin::Round => LyonLineJoin::Round,
        LineJoin::Bevel => LyonLineJoin::Bevel,
    }
}

pub(crate) struct PathPipeline {
    pub(crate) vertex_buffer: wgpu::Buffer,
    pub(crate) index_buffer: wgpu::Buffer,
    vertex_capacity: usize,
    index_capacity: usize,
    pub(crate) pipeline: wgpu::RenderPipeline,
}

impl PathPipeline {
    pub(crate) fn new(
        device: &Device,
        surface_format: wgpu::TextureFormat,
        viewport_bgl: &wgpu::BindGroupLayout,
    ) -> Self {
        const INITIAL_VERTEX_CAPACITY: usize = 1024;
        const INITIAL_INDEX_CAPACITY: usize = 3072;

        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rsx-path-vb"),
            size: (std::mem::size_of::<PathVertex>() * INITIAL_VERTEX_CAPACITY) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rsx-path-ib"),
            size: (std::mem::size_of::<u32>() * INITIAL_INDEX_CAPACITY) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let shader_source = [include_str!("viewport.wgsl"), include_str!("path.wgsl")].concat();
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rsx-path-shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rsx-path-pipeline-layout"),
            bind_group_layouts: &[Some(viewport_bgl)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rsx-path-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<PathVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            offset: 0,
                            shader_location: 0,
                            format: wgpu::VertexFormat::Float32x2,
                        },
                        wgpu::VertexAttribute {
                            offset: 8,
                            shader_location: 1,
                            format: wgpu::VertexFormat::Float32x4,
                        },
                    ],
                }],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 4,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        });

        Self {
            vertex_buffer,
            index_buffer,
            vertex_capacity: INITIAL_VERTEX_CAPACITY,
            index_capacity: INITIAL_INDEX_CAPACITY,
            pipeline,
        }
    }

    pub(crate) fn ensure_capacity(
        &mut self,
        device: &Device,
        vertex_count: usize,
        index_count: usize,
    ) {
        if vertex_count > self.vertex_capacity {
            let new_cap = (vertex_count * 2).max(self.vertex_capacity * 2);
            self.vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("rsx-path-vb"),
                size: (std::mem::size_of::<PathVertex>() * new_cap) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.vertex_capacity = new_cap;
        }
        if index_count > self.index_capacity {
            let new_cap = (index_count * 2).max(self.index_capacity * 2);
            self.index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("rsx-path-ib"),
                size: (std::mem::size_of::<u32>() * new_cap) as u64,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.index_capacity = new_cap;
        }
    }
}
