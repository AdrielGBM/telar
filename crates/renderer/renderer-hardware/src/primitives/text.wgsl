struct Viewport {
    size: vec2<f32>,
    _pad: vec2<f32>,
}

struct TextInstance {
    dest_rect: vec4<f32>,
    uv_min:    vec2<f32>,
    uv_max:    vec2<f32>,
}

@group(0) @binding(0) var<uniform>          viewport:      Viewport;
@group(0) @binding(1) var<storage, read>    instances:     array<TextInstance>;
@group(1) @binding(0) var                   atlas_texture: texture_2d<f32>;
@group(1) @binding(1) var                   atlas_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0)       uv:       vec2<f32>,
}

@vertex
fn vs_main(
    @builtin(vertex_index)   vertex_index:   u32,
    @builtin(instance_index) instance_index: u32,
) -> VertexOutput {
    var offsets = array<vec2<f32>, 6>(
        vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0),
        vec2(1.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 1.0),
    );
    let inst = instances[instance_index];
    let off  = offsets[vertex_index];

    let px   = inst.dest_rect.x + off.x * inst.dest_rect.z;
    let py   = inst.dest_rect.y + off.y * inst.dest_rect.w;
    let ndcx = px / viewport.size.x * 2.0 - 1.0;
    let ndcy = 1.0 - py / viewport.size.y * 2.0;

    var out: VertexOutput;
    out.position = vec4(ndcx, ndcy, 0.0, 1.0);
    out.uv       = inst.uv_min + off * (inst.uv_max - inst.uv_min);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(atlas_texture, atlas_sampler, in.uv);
}
