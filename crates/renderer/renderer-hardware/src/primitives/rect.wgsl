struct RectInstance {
    rect: vec4<f32>,         
    radii: vec4<f32>,        
    fill_color: vec4<f32>,   
    stroke_color: vec4<f32>,
    stroke_width: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
}

@group(0) @binding(1) var<storage, read> instances: array<RectInstance>;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) local_pos: vec2<f32>,
    @location(1) half_size: vec2<f32>,
    @location(2) radii: vec4<f32>,
    @location(3) fill_color: vec4<f32>,
    @location(4) stroke_color: vec4<f32>,
    @location(5) stroke_width: f32,
}

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
) -> VertexOutput {
    var offsets = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 1.0),
    );

    let inst = instances[instance_index];
    let uv = offsets[vertex_index];

    let px = inst.rect.x + uv.x * inst.rect.z;
    let py = inst.rect.y + uv.y * inst.rect.w;
    let ndc_x = px / viewport.size.x * 2.0 - 1.0;
    let ndc_y = 1.0 - py / viewport.size.y * 2.0;

    var out: VertexOutput;
    out.clip_position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.local_pos = vec2<f32>((uv.x - 0.5) * inst.rect.z, (uv.y - 0.5) * inst.rect.w);
    out.half_size = vec2<f32>(inst.rect.z * 0.5, inst.rect.w * 0.5);
    out.radii = inst.radii;
    out.fill_color = inst.fill_color;
    out.stroke_color = inst.stroke_color;
    out.stroke_width = inst.stroke_width;
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
    let dist = sdf_rounded_rect(in.local_pos, in.half_size, in.radii);
    let aa = 0.5;

    let rect_mask = smoothstep(aa, -aa, dist);

    let has_stroke = in.stroke_width > 0.0 && in.stroke_color.a > 0.0;

    var fill_mask: f32;
    var stroke_mask: f32;

    if has_stroke {
        let inner_mask = smoothstep(-in.stroke_width + aa, -in.stroke_width - aa, dist);
        fill_mask = rect_mask * inner_mask * in.fill_color.a;
        stroke_mask = rect_mask * (1.0 - inner_mask) * in.stroke_color.a;
    } else {
        fill_mask = rect_mask * in.fill_color.a;
        stroke_mask = 0.0;
    }

    let out_a = fill_mask + stroke_mask * (1.0 - fill_mask);
    if out_a < 0.001 {
        discard;
    }

    let out_rgb = (in.fill_color.rgb * fill_mask
        + in.stroke_color.rgb * stroke_mask * (1.0 - fill_mask)) / out_a;
    return vec4<f32>(out_rgb, out_a);
}
