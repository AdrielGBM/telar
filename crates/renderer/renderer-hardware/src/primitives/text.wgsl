struct TextInstance {
    dest_rect: vec4<f32>,
    uv_min:    vec2<f32>,
    uv_max:    vec2<f32>,
    color:     vec4<f32>,
}

@group(1) @binding(0) var<storage, read>    instances:     array<TextInstance>;
@group(2) @binding(0) var                   atlas_texture: texture_2d<f32>;
@group(2) @binding(1) var                   atlas_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0)       uv:       vec2<f32>,
    @location(1)       color:    vec4<f32>,
}

@vertex
fn vs_main(
    @builtin(vertex_index)   vertex_index:   u32,
    @builtin(instance_index) instance_index: u32,
) -> VertexOutput {
    let inst = instances[instance_index];
    let off  = quad_uv(vertex_index);

    let px   = inst.dest_rect.x + off.x * inst.dest_rect.z;
    let py   = inst.dest_rect.y + off.y * inst.dest_rect.w;
    let ndc  = to_ndc(px, py);

    var out: VertexOutput;
    out.position = vec4(ndc.x, ndc.y, 0.0, 1.0);
    out.uv       = inst.uv_min + off * (inst.uv_max - inst.uv_min);
    out.color    = inst.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let rgba = textureSample(atlas_texture, atlas_sampler, in.uv) * in.color;
    // Premultiply alpha: pipeline uses PREMULTIPLIED_ALPHA_BLENDING, so src.rgb must already be multiplied by src.a.
    return vec4(rgba.rgb * rgba.a, rgba.a);
}
