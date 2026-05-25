struct Viewport {
    size: vec2<f32>,
    offset: vec2<f32>,
}
@group(0) @binding(0) var<uniform> viewport: Viewport;

struct CompositeParams {
    rect: vec4<f32>,
    alpha: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
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
    let ndx = (px - viewport.offset.x) / viewport.size.x * 2.0 - 1.0;
    let ndy = 1.0 - (py - viewport.offset.y) / viewport.size.y * 2.0;
    var out: VertexOutput;
    out.position = vec4<f32>(ndx, ndy, 0.0, 1.0);
    out.uv = uv;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(src_texture, src_sampler, in.uv);
    return color * params.alpha;
}
