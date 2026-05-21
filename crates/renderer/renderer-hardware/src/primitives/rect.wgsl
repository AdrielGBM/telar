struct RectInstance {
    rect: vec4<f32>,
    radii: vec4<f32>,
    fill_type: u32,
    _pad_ft_0: u32,
    _pad_ft_1: u32,
    _pad_ft_2: u32,
    fill_color: vec4<f32>,
    grad_p0: vec2<f32>,
    grad_p1: vec2<f32>,
    grad_radius: f32,
    grad_stop_count: u32,
    _pad_g: vec2<f32>,
    grad_positions: vec4<f32>,
    grad_colors_0: vec4<f32>,
    grad_colors_1: vec4<f32>,
    grad_colors_2: vec4<f32>,
    grad_colors_3: vec4<f32>,
    stroke_color: vec4<f32>,
    stroke_width: f32,
    _pad_0: f32,
    _pad_1: f32,
    _pad_2: f32,
}

@group(1) @binding(0) var<storage, read> instances: array<RectInstance>;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) local_pos: vec2<f32>,
    @location(1) half_size: vec2<f32>,
    @location(2) world_pos: vec2<f32>,
    @location(3) @interpolate(flat) instance_index: u32,
}

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
) -> VertexOutput {
    let inst = instances[instance_index];
    let uv = quad_uv(vertex_index);

    let px = inst.rect.x + uv.x * inst.rect.z;
    let py = inst.rect.y + uv.y * inst.rect.w;
    let ndc = to_ndc(px, py);

    var out: VertexOutput;
    out.clip_position = vec4<f32>(ndc.x, ndc.y, 0.0, 1.0);
    out.local_pos = vec2<f32>((uv.x - 0.5) * inst.rect.z, (uv.y - 0.5) * inst.rect.w);
    out.half_size = vec2<f32>(inst.rect.z * 0.5, inst.rect.w * 0.5);
    out.world_pos = vec2<f32>(px, py);
    out.instance_index = instance_index;
    return out;
}

fn sdf_rounded_rect(p: vec2<f32>, b: vec2<f32>, radii: vec4<f32>) -> f32 {
    let r = select(
        select(radii.x, radii.y, p.x > 0.0),
        select(radii.w, radii.z, p.x > 0.0),
        p.y > 0.0,
    );
    let q = abs(p) - b + r;
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let inst = instances[in.instance_index];
    let dist = sdf_rounded_rect(in.local_pos, in.half_size, inst.radii);
    let aa = 0.5;

    let rect_mask = smoothstep(aa, -aa, dist);

    let grad_colors = array<vec4<f32>, 4>(
        inst.grad_colors_0,
        inst.grad_colors_1,
        inst.grad_colors_2,
        inst.grad_colors_3,
    );

    var fill_color: vec4<f32>;
    if inst.fill_type == 1u {
        let dir = inst.grad_p1 - inst.grad_p0;
        let len_sq = dot(dir, dir);
        var t = 0.0;
        if len_sq > 0.0001 {
            t = dot(in.world_pos - inst.grad_p0, dir) / len_sq;
        }
        fill_color = sample_gradient(t, inst.grad_positions, grad_colors, inst.grad_stop_count);
    } else if inst.fill_type == 2u {
        var t = 0.0;
        if inst.grad_radius > 0.0001 {
            t = length(in.world_pos - inst.grad_p0) / inst.grad_radius;
        }
        fill_color = sample_gradient(t, inst.grad_positions, grad_colors, inst.grad_stop_count);
    } else {
        fill_color = inst.fill_color;
    }

    let has_stroke = inst.stroke_width > 0.0 && inst.stroke_color.a > 0.0;

    var fill_mask: f32;
    var stroke_mask: f32;

    if has_stroke {
        let inner_mask = smoothstep(-inst.stroke_width + aa, -inst.stroke_width - aa, dist);
        fill_mask = rect_mask * inner_mask * fill_color.a;
        stroke_mask = rect_mask * (1.0 - inner_mask) * inst.stroke_color.a;
    } else {
        fill_mask = rect_mask * fill_color.a;
        stroke_mask = 0.0;
    }

    let out_a = fill_mask + stroke_mask * (1.0 - fill_mask);
    if out_a < 0.001 {
        discard;
    }

    let out_rgb = (fill_color.rgb * fill_mask
        + inst.stroke_color.rgb * stroke_mask * (1.0 - fill_mask)) / out_a;
    return vec4<f32>(out_rgb, out_a);
}
