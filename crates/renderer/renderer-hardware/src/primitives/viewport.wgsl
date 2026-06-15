struct Viewport {
    size: vec2<f32>,
    offset: vec2<f32>,
    scale: f32,
    _pad: f32,
}

@group(0) @binding(0) var<uniform> viewport: Viewport;

fn to_ndc(px: f32, py: f32) -> vec2<f32> {
    return vec2<f32>(
        (px * viewport.scale - viewport.offset.x) / viewport.size.x * 2.0 - 1.0,
        1.0 - (py * viewport.scale - viewport.offset.y) / viewport.size.y * 2.0,
    );
}

fn quad_uv(vertex_index: u32) -> vec2<f32> {
    var offsets = array<vec2<f32>, 6>(
        vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0),
        vec2(1.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 1.0),
    );
    return offsets[vertex_index];
}

fn sample_gradient(
    t: f32,
    positions: vec4<f32>,
    colors: array<vec4<f32>, 4>,
    stop_count: u32,
) -> vec4<f32> {
    let tc = clamp(t, 0.0, 1.0);
    if stop_count == 0u { return vec4<f32>(0.0); }
    if stop_count == 1u { return colors[0]; }
    var result = colors[0];
    for (var i = 1u; i < stop_count; i++) {
        let p0 = positions[i - 1u];
        let p1 = positions[i];
        if tc <= p1 {
            if p1 <= p0 {
                result = colors[i - 1u];
            } else {
                let local_t = (tc - p0) / (p1 - p0);
                result = mix(colors[i - 1u], colors[i], local_t);
            }
            return result;
        }
        result = colors[i];
    }
    return result;
}
