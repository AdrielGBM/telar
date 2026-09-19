// Appended to composite.wgsl for a layer whose blend mode reads what is beneath it. The formulas are tiny-skia's (and through it Skia's) premultiplied forms, so the software backend and this one compute the same thing.

// A copy of the target this composite draws into, taken just before it, at the same pixel size.
@group(2) @binding(0) var backdrop_texture: texture_2d<f32>;

// Values match `blend_shader_mode` on the Rust side.
override MODE: u32 = 0u;

fn inv(v: f32) -> f32 {
    return 1.0 - v;
}

fn separable(s: f32, d: f32, sa: f32, da: f32) -> f32 {
    let rest = s * inv(da) + d * inv(sa);
    switch MODE {
        case 1u: {
            return rest + s * d;
        }
        case 2u: {
            if 2.0 * d <= da {
                return rest + 2.0 * s * d;
            }
            return rest + sa * da - 2.0 * (da - d) * (sa - s);
        }
        case 3u: {
            return s + d - max(s * da, d * sa);
        }
        case 4u: {
            return s + d - min(s * da, d * sa);
        }
        case 5u: {
            if d == 0.0 {
                return s * inv(da);
            }
            if s == sa {
                return s + d * inv(sa);
            }
            return sa * min(da, (d * sa) / (sa - s)) + rest;
        }
        case 6u: {
            if d == da {
                return d + s * inv(da);
            }
            if s == 0.0 {
                return d * inv(sa);
            }
            return sa * (da - min(da, (da - d) * sa / s)) + rest;
        }
        case 7u: {
            if 2.0 * s <= sa {
                return rest + 2.0 * s * d;
            }
            return rest + sa * da - 2.0 * (da - d) * (sa - s);
        }
        case 8u: {
            var m = 0.0;
            if da > 0.0 {
                m = d / da;
            }
            let s2 = 2.0 * s;
            let m4 = 4.0 * m;
            let dark_src = d * (sa + (s2 - sa) * (1.0 - m));
            let dark_dst = (m4 * m4 + m4) * (m - 1.0) + 7.0 * m;
            let lite_dst = sqrt(m) - m;
            var lite_src = d * sa + da * (s2 - sa) * lite_dst;
            if 4.0 * d <= da {
                lite_src = d * sa + da * (s2 - sa) * dark_dst;
            }
            if s2 <= sa {
                return rest + dark_src;
            }
            return rest + lite_src;
        }
        case 9u: {
            return s + d - 2.0 * min(s * da, d * sa);
        }
        case 10u: {
            return s + d - 2.0 * s * d;
        }
        default: {
            return s + d * inv(sa);
        }
    }
}

fn lum(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.30, 0.59, 0.11));
}

fn sat(c: vec3<f32>) -> f32 {
    return max(c.r, max(c.g, c.b)) - min(c.r, min(c.g, c.b));
}

fn set_sat(c: vec3<f32>, s: f32) -> vec3<f32> {
    let mn = min(c.r, min(c.g, c.b));
    let range = max(c.r, max(c.g, c.b)) - mn;
    if range == 0.0 {
        return vec3<f32>(0.0);
    }
    return (c - vec3<f32>(mn)) * s / range;
}

fn set_lum(c: vec3<f32>, l: f32) -> vec3<f32> {
    return c + vec3<f32>(l - lum(c));
}

// Kept as tiny-skia has it, the first test included, since matching it is the point. Except where it divides zero by zero: a grey has its lightness at both extremes, and tiny-skia lets that NaN fall to 0 or not depending on how its last bit rounded, where the grey itself is the answer.
fn clip_color(c: vec3<f32>, a: f32) -> vec3<f32> {
    let mn = min(c.r, min(c.g, c.b));
    let mx = max(c.r, max(c.g, c.b));
    let l = lum(c);
    var out = c;
    if !(mx >= 0.0) && l != mn {
        out = vec3<f32>(l) + (out - vec3<f32>(l)) * l / (l - mn);
    }
    if mx > a && mx != l {
        out = vec3<f32>(l) + (out - vec3<f32>(l)) * (a - l) / (mx - l);
    }
    return max(out, vec3<f32>(0.0));
}

fn non_separable(s: vec4<f32>, d: vec4<f32>) -> vec3<f32> {
    let sa = s.a;
    let da = d.a;
    var mixed: vec3<f32>;
    switch MODE {
        case 11u: {
            mixed = set_lum(set_sat(s.rgb * sa, sat(d.rgb) * sa), lum(d.rgb) * sa);
        }
        case 12u: {
            mixed = set_lum(set_sat(d.rgb * sa, sat(s.rgb) * da), lum(d.rgb) * sa);
        }
        case 13u: {
            mixed = set_lum(s.rgb * da, lum(d.rgb) * sa);
        }
        default: {
            mixed = set_lum(d.rgb * sa, lum(s.rgb) * da);
        }
    }
    mixed = clip_color(mixed, sa * da);
    return s.rgb * inv(da) + d.rgb * inv(sa) + mixed;
}

fn blended(s: vec4<f32>, d: vec4<f32>) -> vec3<f32> {
    if MODE >= 11u {
        return non_separable(s, d);
    }
    return vec3<f32>(
        separable(s.r, d.r, s.a, d.a),
        separable(s.g, d.g, s.a, d.a),
        separable(s.b, d.b, s.a, d.a),
    );
}

// The pipeline blends `One, OneMinusSrcAlpha`, which adds `d * (1 - coverage * sa)` to what this returns. So returning the blend's own share, `coverage * (result - d * (1 - sa))`, lands `mix(d, result, coverage)`: tiny-skia's lerp by coverage, with the backdrop read by the blend unit where it stands.
@fragment
fn fs_blend(in: VertexOutput) -> @location(0) vec4<f32> {
    let s = source_color(in.uv);
    let coverage = composite_coverage(in.uv);
    let d = textureLoad(backdrop_texture, vec2<i32>(in.position.xy), 0);
    let own = blended(s, d) - d.rgb * inv(s.a);
    return vec4<f32>(own, s.a) * coverage;
}
