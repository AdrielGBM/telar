use std::sync::Arc;

use renderer_cache::{Cache, Policy};

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
pub(crate) struct PathFillData {
    pub fill_type: u32,
    pub _pad: [u32; 3],
    pub fill_color: [f32; 4],
    pub grad_p0: [f32; 2],
    pub grad_p1: [f32; 2],
    pub grad_radius: f32,
    pub grad_stop_count: u32,
    pub _pad2: [f32; 2],
    pub grad_positions: [f32; 8],
    pub grad_colors: [[f32; 4]; 8],
}

impl PathFillData {
    pub(crate) fn from_fill_style(fill: renderer_core::Paint) -> Self {
        let enc =
            super::encode_fill_style::<8>(&fill, geometry_core::Transform::IDENTITY.to_array());
        Self {
            fill_type: enc.fill_type,
            _pad: [0; 3],
            fill_color: enc.fill_color,
            grad_p0: enc.grad_p0,
            grad_p1: enc.grad_p1,
            grad_radius: enc.grad_radius,
            grad_stop_count: enc.grad_stop_count,
            _pad2: [0.0; 2],
            grad_positions: enc.grad_positions,
            grad_colors: enc.grad_colors,
        }
    }
}

pub(crate) struct FillDataBuffer {
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
    pub buffer: wgpu::Buffer,
    capacity: usize,
}

impl FillDataBuffer {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("telar-path-fill-bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(
                        std::mem::size_of::<PathFillData>() as u64
                    ),
                },
                count: None,
            }],
        });
        let (buffer, bind_group) =
            Self::create_buffer_and_bind_group(device, &bind_group_layout, 64);
        Self {
            bind_group_layout,
            bind_group,
            buffer,
            capacity: 64,
        }
    }

    fn create_buffer_and_bind_group(
        device: &wgpu::Device,
        bind_group_layout: &wgpu::BindGroupLayout,
        capacity: usize,
    ) -> (wgpu::Buffer, wgpu::BindGroup) {
        let size = (std::mem::size_of::<PathFillData>() * capacity.max(1)) as u64;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("telar-path-fill-buf"),
            size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("telar-path-fill-bg"),
            layout: bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });
        (buffer, bind_group)
    }

    pub(crate) fn ensure_capacity(&mut self, device: &wgpu::Device, count: usize) {
        if count > self.capacity {
            let new_cap = (count * 2).max(self.capacity * 2);
            let (buffer, bind_group) =
                Self::create_buffer_and_bind_group(device, &self.bind_group_layout, new_cap);
            self.buffer = buffer;
            self.bind_group = bind_group;
            self.capacity = new_cap;
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct PathVertex {
    pub position: [f32; 2],
    pub fill_index: u32,
    pub _pad: u32,
}

#[derive(Hash, PartialEq, Eq, Clone, Copy)]
struct FillGeomKey {
    id: u64,
    even_odd: bool,
}

#[derive(Hash, PartialEq, Eq, Clone, Copy)]
struct StrokeGeomKey {
    id: u64,
    width_bits: u32,
    cap: u8,
    join: u8,
}

struct CachedGeom {
    positions: Vec<[f32; 2]>,
    indices: Vec<u32>,
}

fn geom_bytes(geom: &CachedGeom) -> usize {
    geom.positions.len() * size_of::<[f32; 2]>() + geom.indices.len() * size_of::<u32>()
}

/// Tessellated fill and stroke geometry, keyed by the path and the options it was tessellated under.
///
/// Two [`Cache`]s where this used to be two maps, two `VecDeque`s of `(key, frame)`, a frame counter and a
/// `last_frame` per entry — machinery that existed to answer "has anything used this lately", including the subtle
/// part where a queued eviction had to be skipped if the entry had been touched since. It also bounded nothing but
/// age: a frame full of complex paths could tessellate without limit.
pub(crate) struct PathTessCache {
    fill: Cache<FillGeomKey, CachedGeom>,
    stroke: Cache<StrokeGeomKey, CachedGeom>,
}

impl PathTessCache {
    pub(crate) fn new(policy: Policy) -> Self {
        Self {
            fill: Cache::new(policy, geom_bytes),
            stroke: Cache::new(policy, geom_bytes),
        }
    }

    pub(crate) fn stats(&self) -> [renderer_cache::CacheStat; 2] {
        [
            self.fill.stat("gpu.path_fill"),
            self.stroke.stat("gpu.path_stroke"),
        ]
    }
}

fn emit_cached_geom(
    geom: &CachedGeom,
    fill_index: u32,
    out_vertices: &mut Vec<PathVertex>,
    out_indices: &mut Vec<u32>,
) {
    let vertex_base = out_vertices.len() as u32;
    for &pos in &geom.positions {
        out_vertices.push(PathVertex {
            position: pos,
            fill_index,
            _pad: 0,
        });
    }
    for &idx in &geom.indices {
        out_indices.push(vertex_base + idx);
    }
}

fn build_lyon_path(data: &PathData) -> Path {
    let mut builder = Path::builder();
    let mut in_path = false;

    for verb in data.verbs() {
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
    out_fill_data: &mut Vec<PathFillData>,
) {
    let id = data.id;

    let mut lyon_path: Option<Path> = None;

    if let Some(fill_style) = style.fill {
        let fill_key = FillGeomKey {
            id,
            even_odd: style.fill_rule == FillRule::EvenOdd,
        };

        if !cache.fill.contains(&fill_key) {
            let lyon_path = lyon_path.get_or_insert_with(|| build_lyon_path(data));
            let mut geometry: VertexBuffers<[f32; 2], u32> = VertexBuffers::new();
            let mut tessellator = FillTessellator::new();
            let lyon_fill_rule = if fill_key.even_odd {
                lyon::tessellation::FillRule::EvenOdd
            } else {
                lyon::tessellation::FillRule::NonZero
            };
            let options = FillOptions::default().with_fill_rule(lyon_fill_rule);
            match tessellator.tessellate_path(
                &*lyon_path,
                &options,
                &mut BuffersBuilder::new(&mut geometry, |v: FillVertex| {
                    [v.position().x, v.position().y]
                }),
            ) {
                Ok(_) => {
                    cache.fill.insert(
                        fill_key,
                        CachedGeom {
                            positions: geometry.vertices,
                            indices: geometry.indices,
                        },
                    );
                }
                Err(e) => tracing::warn!("path fill tessellation failed: {e}"),
            }
        }

        let fill_data_entry = PathFillData::from_fill_style(fill_style);
        out_fill_data.push(fill_data_entry);
        let fill_index = (out_fill_data.len() - 1) as u32;

        if let Some(geom) = cache.fill.get(&fill_key) {
            emit_cached_geom(geom, fill_index, out_vertices, out_indices);
        }
    }

    if let Some(s) = style.stroke {
        let stroke_key = StrokeGeomKey {
            id,
            width_bits: s.width.to_bits(),
            cap: s.cap as u8,
            join: s.join as u8,
        };

        if !cache.stroke.contains(&stroke_key) {
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
            match tessellator.tessellate_path(
                &*lyon_path,
                &options,
                &mut BuffersBuilder::new(&mut geometry, |v: StrokeVertex| {
                    [v.position().x, v.position().y]
                }),
            ) {
                Ok(_) => {
                    cache.stroke.insert(
                        stroke_key,
                        CachedGeom {
                            positions: geometry.vertices,
                            indices: geometry.indices,
                        },
                    );
                }
                Err(e) => tracing::warn!("path stroke tessellation failed: {e}"),
            }
        }

        if let Some(geom) = cache.stroke.get(&stroke_key) {
            let stroke_fill_index = out_fill_data.len() as u32;
            // The stroke gets its own fill-data entry, so it resolves its paint the same way the fill does. Flattening it to `solid_color()` here silently painted a gradient stroke in its first stop.
            out_fill_data.push(PathFillData::from_fill_style(s.paint));
            emit_cached_geom(geom, stroke_fill_index, out_vertices, out_indices);
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
    pub(crate) fill_data: FillDataBuffer,
}

impl PathPipeline {
    pub(crate) fn new(
        device: &Device,
        surface_format: wgpu::TextureFormat,
        viewport_bgl: &wgpu::BindGroupLayout,
        cache: Option<&wgpu::PipelineCache>,
        msaa_samples: u32,
    ) -> Self {
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("telar-path-vb"),
            size: (std::mem::size_of::<PathVertex>() * 1024) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("telar-path-ib"),
            size: (std::mem::size_of::<u32>() * 3072) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let shader_source = [include_str!("viewport.wgsl"), include_str!("path.wgsl")].concat();
        let fill_data = FillDataBuffer::new(device);

        let path_vertex_layout = wgpu::VertexBufferLayout {
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
                    format: wgpu::VertexFormat::Uint32,
                },
            ],
        };

        let pipeline = super::create_render_pipeline(
            device,
            "path",
            &shader_source,
            &[viewport_bgl, &fill_data.bind_group_layout],
            &[Some(path_vertex_layout)],
            surface_format,
            msaa_samples,
            cache,
        );

        Self {
            vertex_buffer,
            index_buffer,
            vertex_capacity: 1024,
            index_capacity: 3072,
            pipeline,
            fill_data,
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
                label: Some("telar-path-vb"),
                size: (std::mem::size_of::<PathVertex>() * new_cap) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.vertex_capacity = new_cap;
        }
        if index_count > self.index_capacity {
            let new_cap = (index_count * 2).max(self.index_capacity * 2);
            self.index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("telar-path-ib"),
                size: (std::mem::size_of::<u32>() * new_cap) as u64,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.index_capacity = new_cap;
        }
    }
}
