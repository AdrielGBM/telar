struct Viewport {
    size: vec2<f32>,
    offset: vec2<f32>,
    scale: f32,
    // Three scalar pads (not a vec3) so clip_rect lands at offset 32, matching the Rust #[repr(C)] layout byte-for-byte.
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
    // Active rounded-clip rect in logical/world space (x, y, w, h); zero size means no clip.
    clip_rect: vec4<f32>,
    // Corner radius of the active rounded clip; 0 disables the SDF clip mask.
    clip_radius: f32,
    // Scalar pads (not a vec3, whose 16-byte alignment would inflate the struct size) keep the WGSL size at 64 bytes, matching the Rust #[repr(C)] struct.
    _clip_pad0: f32,
    _clip_pad1: f32,
    _clip_pad2: f32,
}

@group(0) @binding(0) var<uniform> viewport: Viewport;

// Returns distance to a rounded rect in world space; negative inside, positive outside.
fn sdf_clip_rounded_rect(world_pos: vec2<f32>, clip: vec4<f32>, radius: f32) -> f32 {
    let center = clip.xy + clip.zw * 0.5;
    let half = clip.zw * 0.5 - vec2<f32>(radius);
    let p = abs(world_pos - center) - half;
    return length(max(p, vec2<f32>(0.0))) + min(max(p.x, p.y), 0.0) - radius;
}

// Discards the fragment when it falls outside the viewport's active rounded clip.
fn apply_clip_sdf(world_pos: vec2<f32>) -> bool {
    if viewport.clip_radius > 0.0 {
        let clip_dist = sdf_clip_rounded_rect(world_pos, viewport.clip_rect, viewport.clip_radius);
        return clip_dist > 0.5;
    }
    return false;
}

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
