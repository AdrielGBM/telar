struct Viewport {
    size: vec2<f32>,
    _pad: vec2<f32>,
}

struct TextUniforms {
    rect: vec4<f32>,
}

@group(0) @binding(0) var<uniform> viewport: Viewport;
@group(0) @binding(1) var<uniform> text_uniforms: TextUniforms;
@group(1) @binding(0) var t_text: texture_2d<f32>;
@group(1) @binding(1) var s_text: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VertexOutput {
    var uvs = array<vec2<f32>, 6>(
        vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0),
        vec2(1.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 1.0),
    );
    let uv = uvs[vi];
    let r = text_uniforms.rect;
    let px = r.x + uv.x * r.z;
    let py = r.y + uv.y * r.w;
    let ndcx = (px / viewport.size.x) * 2.0 - 1.0;
    let ndcy = 1.0 - (py / viewport.size.y) * 2.0;
    var out: VertexOutput;
    out.position = vec4(ndcx, ndcy, 0.0, 1.0);
    out.uv = uv;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(t_text, s_text, in.uv);
}
