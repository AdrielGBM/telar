// Kawase dual-filter up-sample pass (ARM SIGGRAPH 2015). Reuses BlurParams: tex_size is the source texture size; direction and sigma are unused.
struct BlurParams {
    direction: vec2<f32>,
    tex_size: vec2<f32>,
    sigma: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
}

@group(0) @binding(0) var src_texture: texture_2d<f32>;
@group(0) @binding(1) var src_sampler: sampler;
@group(0) @binding(2) var<uniform> params: BlurParams;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VertexOutput {
    var positions = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>( 1.0,  1.0),
    );
    var uvs = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(1.0, 0.0),
    );
    var out: VertexOutput;
    out.position = vec4<f32>(positions[vi], 0.0, 1.0);
    out.uv = uvs[vi];
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let half_texel = vec2<f32>(0.5) / params.tex_size;
    var color =
        textureSample(src_texture, src_sampler, in.uv + vec2<f32>(-2.0 * half_texel.x,  0.0)) +
        textureSample(src_texture, src_sampler, in.uv + vec2<f32>(-half_texel.x,  half_texel.y)) +
        textureSample(src_texture, src_sampler, in.uv + vec2<f32>( 0.0,  2.0 * half_texel.y)) +
        textureSample(src_texture, src_sampler, in.uv + vec2<f32>( half_texel.x,  half_texel.y)) +
        textureSample(src_texture, src_sampler, in.uv + vec2<f32>( 2.0 * half_texel.x,  0.0)) +
        textureSample(src_texture, src_sampler, in.uv + vec2<f32>( half_texel.x, -half_texel.y)) +
        textureSample(src_texture, src_sampler, in.uv + vec2<f32>( 0.0, -2.0 * half_texel.y)) +
        textureSample(src_texture, src_sampler, in.uv + vec2<f32>(-half_texel.x, -half_texel.y));
    return color / 8.0;
}
