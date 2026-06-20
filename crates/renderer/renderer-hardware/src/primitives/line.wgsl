struct LineInstance {
    p1: vec2<f32>,
    p2: vec2<f32>,
    color: vec4<f32>,
    width: f32,
    cap: f32,
    _pad0: f32,
    _pad1: f32,
}

@group(1) @binding(0) var<storage, read> instances: array<LineInstance>;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) screen_pos: vec2<f32>,
    @location(1) p1: vec2<f32>,
    @location(2) p2: vec2<f32>,
    @location(3) color: vec4<f32>,
    @location(4) half_width: f32,
    @location(5) cap: f32,
}

const AA: f32 = 1.0;

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
) -> VertexOutput {
    let inst = instances[instance_index];
    let p1 = inst.p1;
    let p2 = inst.p2;
    let half = inst.width * 0.5 + AA;

    let delta = p2 - p1;
    let len = length(delta);
    var dir: vec2<f32>;
    if len < 0.0001 {
        dir = vec2<f32>(1.0, 0.0);
    } else {
        dir = delta / len;
    }
    let perp = vec2<f32>(-dir.y, dir.x);


    var verts = array<vec2<f32>, 6>(
        p1 - dir * half + perp * half,
        p2 + dir * half + perp * half,
        p1 - dir * half - perp * half,
        p1 - dir * half - perp * half,
        p2 + dir * half + perp * half,
        p2 + dir * half - perp * half,
    );

    let corner = verts[vertex_index];
    let ndc = to_ndc(corner.x, corner.y);

    var out: VertexOutput;
    out.clip_position = vec4<f32>(ndc.x, ndc.y, 0.0, 1.0);
    out.screen_pos = corner;
    out.p1 = p1;
    out.p2 = p2;
    out.color = inst.color;
    out.half_width = inst.width * 0.5;
    out.cap = inst.cap;
    return out;
}

fn sdf_segment(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let ab = b - a;
    let len_sq = dot(ab, ab);
    if len_sq < 0.00001 {
        return length(p - a);
    }
    let t = clamp(dot(p - a, ab) / len_sq, 0.0, 1.0);
    return length(p - (a + t * ab));
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    if apply_clip_sdf(in.screen_pos) { discard; }
    let ab = in.p2 - in.p1;
    let len = length(ab);
    let dir = ab / max(len, 0.0001);
    let ap = in.screen_pos - in.p1;
    let along = dot(ap, dir);
    let lateral = abs(dot(ap, vec2<f32>(-dir.y, dir.x)));

    if in.cap < 0.5 {
        if along < 0.0 || along > len {
            discard;
        }
        let mask = smoothstep(in.half_width + AA, in.half_width - AA, lateral) * in.color.a;
        if mask < 0.001 { discard; }
        return vec4<f32>(in.color.rgb, mask);
    } else if in.cap < 1.5 {
        let dist = sdf_segment(in.screen_pos, in.p1, in.p2);
        let mask = smoothstep(in.half_width + AA, in.half_width - AA, dist) * in.color.a;
        if mask < 0.001 { discard; }
        return vec4<f32>(in.color.rgb, mask);
    } else {
        if along < -in.half_width || along > len + in.half_width {
            discard;
        }
        let mask = smoothstep(in.half_width + AA, in.half_width - AA, lateral) * in.color.a;
        if mask < 0.001 { discard; }
        return vec4<f32>(in.color.rgb, mask);
    }
}
