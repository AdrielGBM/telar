struct Viewport {
    size: vec2<f32>,
    offset: vec2<f32>,
    scale: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
    clip_rect: vec4<f32>,
    clip_radius: f32,
    _clip_pad0: f32,
    _clip_pad1: f32,
    _clip_pad2: f32,
}
@group(0) @binding(0) var<uniform> viewport: Viewport;

struct CompositeParams {
    rect: vec4<f32>,
    alpha: f32,
    clip_radius: f32,
    content_uv_scale: vec2<f32>,
    // The rounded clip enclosing this composite (x, y, w, h), in the same logical space as `rect`.
    outer_clip_rect: vec4<f32>,
    // Corner radius of that enclosing clip; 0 disables its mask.
    outer_clip_radius: f32,
    // Scalar pads (not a vec3, whose 16-byte alignment would inflate the struct size) keep the WGSL size at 64 bytes, matching the Rust #[repr(C)] struct.
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
}

fn sdf_rrect(p: vec2<f32>, half_size: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - half_size + r;
    return length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - r;
}

// Coverage of a rounded rect at `pos`, both in the logical space the vertex shader builds its quad in: 1 well inside, 0 well outside, antialiased across the edge.
fn rrect_coverage(pos: vec2<f32>, rect: vec4<f32>, radius: f32) -> f32 {
    let half_size = rect.zw * 0.5;
    return smoothstep(0.5, -0.5, sdf_rrect(pos - rect.xy - half_size, half_size, radius));
}

@group(1) @binding(0) var src_texture: texture_2d<f32>;
@group(1) @binding(1) var src_sampler: sampler;
@group(1) @binding(2) var<uniform> params: CompositeParams;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VertexOutput {
    var offsets = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 1.0),
    );
    let uv = offsets[vi];
    let px = params.rect.x + uv.x * params.rect.z;
    let py = params.rect.y + uv.y * params.rect.w;
    let ndx = (px * viewport.scale - viewport.offset.x) / viewport.size.x * 2.0 - 1.0;
    let ndy = 1.0 - (py * viewport.scale - viewport.offset.y) / viewport.size.y * 2.0;
    var out: VertexOutput;
    out.position = vec4<f32>(ndx, ndy, 0.0, 1.0);
    out.uv = uv;
    return out;
}

fn source_color(uv: vec2<f32>) -> vec4<f32> {
    return textureSample(src_texture, src_sampler, uv * params.content_uv_scale) * params.alpha;
}

fn composite_coverage(uv: vec2<f32>) -> f32 {
    var coverage = 1.0;
    let pos = params.rect.xy + uv * params.rect.zw;
    if params.clip_radius > 0.0 {
        coverage = coverage * rrect_coverage(pos, params.rect, params.clip_radius);
    }
    // A composite drawn in a pass of its own carries the enclosing rounded clip here, having left the viewport it was pushed onto behind.
    if params.outer_clip_radius > 0.0 {
        coverage = coverage * rrect_coverage(pos, params.outer_clip_rect, params.outer_clip_radius);
    }
    // One drawn inside the main pass — a blurred shadow — still has that viewport bound, and masks against it like every other draw in the pass.
    if viewport.clip_radius > 0.0 {
        coverage = coverage * rrect_coverage(pos, viewport.clip_rect, viewport.clip_radius);
    }
    return coverage;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return source_color(in.uv) * composite_coverage(in.uv);
}
