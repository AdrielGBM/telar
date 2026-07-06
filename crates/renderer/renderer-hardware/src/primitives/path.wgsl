struct PathFillData {
    fill_type: u32,
    _pad_0: u32,
    _pad_1: u32,
    _pad_2: u32,
    fill_color: vec4<f32>,
    grad_p0: vec2<f32>,
    grad_p1: vec2<f32>,
    grad_radius: f32,
    grad_stop_count: u32,
    _pad2: vec2<f32>,
    // 8 stop offsets packed as two vec4s (core gradients carry up to 8 stops).
    grad_positions_0: vec4<f32>,
    grad_positions_1: vec4<f32>,
    grad_colors_0: vec4<f32>,
    grad_colors_1: vec4<f32>,
    grad_colors_2: vec4<f32>,
    grad_colors_3: vec4<f32>,
    grad_colors_4: vec4<f32>,
    grad_colors_5: vec4<f32>,
    grad_colors_6: vec4<f32>,
    grad_colors_7: vec4<f32>,
}

@group(1) @binding(0) var<storage, read> fill_data: array<PathFillData>;

// Path-only 8-stop variant of the shared sample_gradient in viewport.wgsl (rect/text keep the 4-stop path).
fn sample_gradient8(
    t: f32,
    positions: array<f32, 8>,
    colors: array<vec4<f32>, 8>,
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

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) fill_index: u32,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_pos: vec2<f32>,
    @location(1) @interpolate(flat) fill_index: u32,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    let ndc = to_ndc(in.position.x, in.position.y);
    var out: VertexOutput;
    out.clip_position = vec4<f32>(ndc.x, ndc.y, 0.0, 1.0);
    out.world_pos = in.position;
    out.fill_index = in.fill_index;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    if apply_clip_sdf(in.world_pos) { discard; }
    let fd = fill_data[in.fill_index];
    let grad_colors = array<vec4<f32>, 8>(
        fd.grad_colors_0,
        fd.grad_colors_1,
        fd.grad_colors_2,
        fd.grad_colors_3,
        fd.grad_colors_4,
        fd.grad_colors_5,
        fd.grad_colors_6,
        fd.grad_colors_7,
    );
    let grad_positions = array<f32, 8>(
        fd.grad_positions_0.x, fd.grad_positions_0.y, fd.grad_positions_0.z, fd.grad_positions_0.w,
        fd.grad_positions_1.x, fd.grad_positions_1.y, fd.grad_positions_1.z, fd.grad_positions_1.w,
    );

    var color: vec4<f32>;
    if fd.fill_type == 1u {
        let dir = fd.grad_p1 - fd.grad_p0;
        let len_sq = dot(dir, dir);
        var t = 0.0;
        if len_sq > 0.0001 {
            t = dot(in.world_pos - fd.grad_p0, dir) / len_sq;
        }
        color = sample_gradient8(t, grad_positions, grad_colors, fd.grad_stop_count);
    } else if fd.fill_type == 2u {
        var t = 0.0;
        if fd.grad_radius > 0.0001 {
            t = length(in.world_pos - fd.grad_p0) / fd.grad_radius;
        }
        color = sample_gradient8(t, grad_positions, grad_colors, fd.grad_stop_count);
    } else {
        color = fd.fill_color;
    }

    if color.a < 0.001 {
        discard;
    }
    // Premultiplied output required by PREMULTIPLIED_ALPHA_BLENDING.
    return vec4<f32>(color.rgb * color.a, color.a);
}
