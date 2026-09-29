// Appended to composite.wgsl for a layer shown through a mask: the alpha of the mask source, sampled where this composite lands, multiplies it, and nothing shows outside the source.

struct MaskParams {
    // Where the source's texture lies, in the same logical space as `params.rect`.
    rect: vec4<f32>,
    uv_scale: vec2<f32>,
    _pad: vec2<f32>,
}

@group(2) @binding(0) var mask_texture: texture_2d<f32>;
@group(2) @binding(1) var<uniform> mask: MaskParams;

@fragment
fn fs_mask(in: VertexOutput) -> @location(0) vec4<f32> {
    let pos = params.rect.xy + in.uv * params.rect.zw;
    let mask_uv = (pos - mask.rect.xy) / mask.rect.zw;
    let inside = all(mask_uv >= vec2<f32>(0.0, 0.0)) && all(mask_uv <= vec2<f32>(1.0, 1.0));
    // A level-sampled read, since which fragments read it is decided per fragment.
    let covered = textureSampleLevel(
        mask_texture,
        src_sampler,
        clamp(mask_uv, vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0)) * mask.uv_scale,
        0.0,
    ).a;
    return source_color(in.uv) * composite_coverage(in.uv) * select(0.0, covered, inside);
}
