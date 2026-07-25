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
    shadow_color: vec4<f32>,
    shadow_offset: vec2<f32>,
    shadow_blur: f32,
    shadow_spread: f32,
    transform_abcd: vec4<f32>,
    transform_ef: vec2<f32>,
    _pad_t: vec2<f32>,
    stroke_type: u32,
    _pad_st_0: u32,
    _pad_st_1: u32,
    _pad_st_2: u32,
    stroke_grad_p0: vec2<f32>,
    stroke_grad_p1: vec2<f32>,
    stroke_grad_radius: f32,
    stroke_grad_stop_count: u32,
    _pad_sg: vec2<f32>,
    stroke_grad_positions: vec4<f32>,
    stroke_grad_colors_0: vec4<f32>,
    stroke_grad_colors_1: vec4<f32>,
    stroke_grad_colors_2: vec4<f32>,
    stroke_grad_colors_3: vec4<f32>,
}

@group(1) @binding(0) var<storage, read> instances: array<RectInstance>;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) half_size: vec2<f32>,
    @location(1) world_pos: vec2<f32>,
    @location(2) @interpolate(flat) instance_index: u32,
}

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
) -> VertexOutput {
    let inst = instances[instance_index];
    let uv = quad_uv(vertex_index);

    let a = inst.transform_abcd.x;
    let b = inst.transform_abcd.y;
    let c = inst.transform_abcd.z;
    let d = inst.transform_abcd.w;
    let e = inst.transform_ef.x;
    let f = inst.transform_ef.y;

    let x0 = inst.rect.x;
    let y0 = inst.rect.y;
    let x1 = inst.rect.x + inst.rect.z;
    let y1 = inst.rect.y + inst.rect.w;

    let wx0 = a * x0 + c * y0 + e;
    let wy0 = b * x0 + d * y0 + f;
    let wx1 = a * x1 + c * y0 + e;
    let wy1 = b * x1 + d * y0 + f;
    let wx2 = a * x0 + c * y1 + e;
    let wy2 = b * x0 + d * y1 + f;
    let wx3 = a * x1 + c * y1 + e;
    let wy3 = b * x1 + d * y1 + f;

    let min_wx = min(min(wx0, wx1), min(wx2, wx3));
    let max_wx = max(max(wx0, wx1), max(wx2, wx3));
    let min_wy = min(min(wy0, wy1), min(wy2, wy3));
    let max_wy = max(max(wy0, wy1), max(wy2, wy3));

    let has_shadow = inst.shadow_color.a > 0.0;
    let shadow_ext = select(0.0, inst.shadow_blur * 2.0 + inst.shadow_spread, has_shadow);
    let sx = select(0.0, inst.shadow_offset.x, has_shadow);
    let sy = select(0.0, inst.shadow_offset.y, has_shadow);

    let ext_left   = min(min_wx, min_wx + sx) - shadow_ext;
    let ext_top    = min(min_wy, min_wy + sy) - shadow_ext;
    let ext_right  = max(max_wx, max_wx + sx) + shadow_ext;
    let ext_bottom = max(max_wy, max_wy + sy) + shadow_ext;
    let ext_w = ext_right - ext_left;
    let ext_h = ext_bottom - ext_top;

    let px = ext_left + uv.x * ext_w;
    let py = ext_top  + uv.y * ext_h;
    let ndc = to_ndc(px, py);

    var out: VertexOutput;
    out.clip_position = vec4<f32>(ndc.x, ndc.y, 0.0, 1.0);
    out.half_size  = vec2<f32>(inst.rect.z * 0.5, inst.rect.w * 0.5);
    out.world_pos  = vec2<f32>(px, py);
    out.instance_index = instance_index;
    return out;
}

fn erfc_approx(x: f32) -> f32 {
    let t = 1.0 / (1.0 + 0.3275911 * abs(x));
    let r = t * (0.254829592 + t * (-0.284496736 + t * (1.421413741 + t * (-1.453152027 + t * 1.061405429))));
    let p = r * exp(-x * x);
    return select(2.0 - p, p, x >= 0.0);
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
    if apply_clip_sdf(in.world_pos) { discard; }
    let inst = instances[in.instance_index];

    let a = inst.transform_abcd.x;
    let b = inst.transform_abcd.y;
    let c = inst.transform_abcd.z;
    let d = inst.transform_abcd.w;
    let e = inst.transform_ef.x;
    let f = inst.transform_ef.y;

    let det = a * d - b * c;
    let inv_a =  d / det;
    let inv_b = -b / det;
    let inv_c = -c / det;
    let inv_d =  a / det;
    let inv_e = (c * f - d * e) / det;
    let inv_f = (b * e - a * f) / det;

    let local_cx = inst.rect.x + inst.rect.z * 0.5;
    let local_cy = inst.rect.y + inst.rect.w * 0.5;

    let lx = inv_a * in.world_pos.x + inv_c * in.world_pos.y + inv_e;
    let ly = inv_b * in.world_pos.x + inv_d * in.world_pos.y + inv_f;
    let local_pos = vec2<f32>(lx - local_cx, ly - local_cy);

    let dist = sdf_rounded_rect(local_pos, in.half_size, inst.radii);
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

    let stroke_grad_colors = array<vec4<f32>, 4>(
        inst.stroke_grad_colors_0,
        inst.stroke_grad_colors_1,
        inst.stroke_grad_colors_2,
        inst.stroke_grad_colors_3,
    );

    var stroke_color: vec4<f32>;
    if inst.stroke_type == 1u {
        let dir = inst.stroke_grad_p1 - inst.stroke_grad_p0;
        let len_sq = dot(dir, dir);
        var t = 0.0;
        if len_sq > 0.0001 {
            t = dot(in.world_pos - inst.stroke_grad_p0, dir) / len_sq;
        }
        stroke_color = sample_gradient(t, inst.stroke_grad_positions, stroke_grad_colors, inst.stroke_grad_stop_count);
    } else if inst.stroke_type == 2u {
        var t = 0.0;
        if inst.stroke_grad_radius > 0.0001 {
            t = length(in.world_pos - inst.stroke_grad_p0) / inst.stroke_grad_radius;
        }
        stroke_color = sample_gradient(t, inst.stroke_grad_positions, stroke_grad_colors, inst.stroke_grad_stop_count);
    } else {
        stroke_color = inst.stroke_color;
    }

    // A gradient stroke leaves `stroke_color` transparent in the instance data (its colour lives in the stop slots), so presence is decided by the paint type, not by that alpha.
    let has_stroke = inst.stroke_width > 0.0 && (inst.stroke_type != 0u || inst.stroke_color.a > 0.0);

    var fill_mask: f32;
    var stroke_mask: f32;

    if has_stroke {
        let inner_mask = smoothstep(-inst.stroke_width + aa, -inst.stroke_width - aa, dist);
        fill_mask = rect_mask * inner_mask * fill_color.a;
        stroke_mask = rect_mask * (1.0 - inner_mask) * stroke_color.a;
    } else {
        fill_mask = rect_mask * fill_color.a;
        stroke_mask = 0.0;
    }

    let main_a = fill_mask + stroke_mask * (1.0 - fill_mask);
    let main_rgb = select(
        vec3<f32>(0.0),
        (fill_color.rgb * fill_mask + stroke_color.rgb * stroke_mask * (1.0 - fill_mask)) / main_a,
        main_a > 0.001
    );

    var shadow_a = 0.0;
    var shadow_rgb = vec3<f32>(0.0);
    if inst.shadow_color.a > 0.0 {
        let sw = in.world_pos - inst.shadow_offset;
        let slx = inv_a * sw.x + inv_c * sw.y + inv_e;
        let sly = inv_b * sw.x + inv_d * sw.y + inv_f;
        let shadow_local = vec2<f32>(slx - local_cx, sly - local_cy);

        let spread = inst.shadow_spread;
        let shadow_half = max(in.half_size + spread, vec2<f32>(0.0));
        let max_r = min(shadow_half.x, shadow_half.y);
        let shadow_radii = clamp(inst.radii + spread, vec4<f32>(0.0), vec4<f32>(max_r));
        let shadow_dist = sdf_rounded_rect(shadow_local, shadow_half, shadow_radii);

        let sigma = max(inst.shadow_blur * 0.5, 0.5);
        shadow_a = inst.shadow_color.a * 0.5 * erfc_approx(shadow_dist / (sigma * 1.41421356));
        shadow_rgb = inst.shadow_color.rgb;
    }

    let final_a = main_a + shadow_a * (1.0 - main_a);
    if final_a < 0.001 {
        discard;
    }
    // Premultiplied output required by PREMULTIPLIED_ALPHA_BLENDING: rgb must already be
    // multiplied by alpha so AA edges and shadow falloff blend correctly instead of leaking
    // full-brightness color into the destination.
    let final_rgb = main_rgb * main_a + shadow_rgb * shadow_a * (1.0 - main_a);
    return vec4<f32>(final_rgb, final_a);
}
