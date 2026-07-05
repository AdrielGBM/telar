[logic]
use crate::core::theme::theme;
use crate::shared::demo_svgs::{make_icon, make_logo, make_blurred};

// Each returns an Arc<SvgData> parsed at runtime (the app enables the dynamic-svg feature).
let icon = make_icon();
let logo = make_logo();
let blurred = make_blurred();

[view]
col gap:20
    doc_header kicker:"10 · MEDIA" title:"SVG" desc:"svg renders vector art crisply at any size. Tint a monochrome glyph, keep full-color gradients, or bake a file from disk."

    col gap:8
        text "One source, drawn crisp at every size" size:13 color:ink
        card
            row gap:20 align:end
                svg src:icon width:20 height:20
                svg src:icon width:36 height:36
                svg src:icon width:64 height:64
                svg src:icon width:96 height:96
        code_line code:"svg src:icon width:96 height:96"

    col gap:8
        text "Tint — recolor a monochrome glyph (reads the active theme)" size:13 color:ink
        card
            row gap:20 align:center
                svg src:icon tint:theme().primary width:48 height:48
                svg src:icon tint:theme().success width:48 height:48
                svg src:icon tint:theme().danger width:48 height:48
                svg src:icon tint:theme().purple width:48 height:48
        code_line code:"svg src:icon tint:theme().primary"

    col gap:8
        text "Full-color vectors and a raster fallback for filters" size:13 color:ink
        card
            row gap:24 align:center
                col gap:6 align:center
                    svg src:logo width:88 height:88
                    text "gradient + shapes" size:12 color:muted
                col gap:6 align:center
                    svg src:blurred width:88 height:88
                    text "feGaussianBlur → raster" size:12 color:muted
        code_line code:"svg src:logo width:88 height:88"

    col gap:8
        text "A vector baked from disk at build time (no runtime parser)" size:13 color:ink
        card
            row gap:20 align:center
                svg src:"badge.svg" width:40 height:40
                svg src:"badge.svg" width:72 height:72
                svg src:"badge.svg" fit:cover width:120 height:56
        code_line code:"svg src:'assets/badge.svg' width:72 height:72"

    col gap:8
        text "Attributes" size:13 color:ink
        col gap:6
            prop_row name:"src" values:"Arc<SvgData> · 'path'" about:"Runtime vector, or a baked file path."
            prop_row name:"tint" values:"Color expr" about:"Recolor a glyph, e.g. theme().primary."
            prop_row name:"fit" values:"contain · cover · fill" about:"Aspect handling in the box."
