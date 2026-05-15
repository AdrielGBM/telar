struct PathVertex {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
}

@vertex
fn vs_main(in: PathVertex) -> VertexOutput {
    let ndc_x = in.position.x / viewport.size.x * 2.0 - 1.0;
    let ndc_y = 1.0 - in.position.y / viewport.size.y * 2.0;
    var out: VertexOutput;
    out.clip_position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    if in.color.a < 0.001 {
        discard;
    }
    return in.color;
}
