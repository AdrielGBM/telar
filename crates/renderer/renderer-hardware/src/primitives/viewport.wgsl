struct Viewport {
    size: vec2<f32>,
    _pad: vec2<f32>,
}

@group(0) @binding(0) var<uniform> viewport: Viewport;

fn to_ndc(px: f32, py: f32) -> vec2<f32> {
    return vec2<f32>(
        px / viewport.size.x * 2.0 - 1.0,
        1.0 - py / viewport.size.y * 2.0,
    );
}

fn quad_uv(vertex_index: u32) -> vec2<f32> {
    var offsets = array<vec2<f32>, 6>(
        vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0),
        vec2(1.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 1.0),
    );
    return offsets[vertex_index];
}
