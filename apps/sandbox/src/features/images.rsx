[logic]
use crate::shared::demo_images::{make_checker, make_gradient, make_radial_alpha};
use std::sync::Arc;

// Bitmaps generated pixel-by-pixel at startup — see demo_images.rs. Any Arc<ImageData> works as a src.
let gradient = Arc::new(make_gradient(128, 128));
let checker = Arc::new(make_checker(64, 64, 8));
let alpha = Arc::new(make_radial_alpha(128, 128));

[style]
@swatch
    gap: 6
    align: center

[view]
col gap:20
    doc_header kicker:"MEDIA" title:"Images" desc:"img draws an RGBA bitmap. Feed it an Arc<ImageData> built in Rust, or a quoted path that is decoded and baked into the binary at build time."
    example title:"A procedural bitmap built from raw pixels"
        card
            img src:gradient width:128 height:128
        code_line code:"let gradient = Arc::new(make_gradient(128, 128));   img src:gradient"
    example title:"Scaling filter — Linear smooths, Nearest keeps hard pixels"
        card
            row gap:20 align:end wrap
                col @swatch
                    img src:checker raster:linear width:120 height:120
                    text "Linear" font_size:12 color:theme.muted
                col @swatch
                    img src:checker raster:nearest width:120 height:120
                    text "Nearest" font_size:12 color:theme.muted
        code_line code:"img src:checker raster:nearest   (a 64px bitmap upscaled to 120)"
    example title:"object-fit — how the bitmap fills a non-square box"
        card
            row gap:20 wrap align:start
                col @swatch
                    img src:gradient fit:contain width:150 height:80
                    text "contain (default)" font_size:12 color:theme.muted
                col @swatch
                    img src:gradient fit:cover width:150 height:80
                    text "cover" font_size:12 color:theme.muted
                col @swatch
                    img src:gradient fit:fill width:150 height:80
                    text "fill" font_size:12 color:theme.muted
        code_line code:"img src:gradient fit:cover width:150 height:80"
    example title:"A PNG baked from disk at build time"
        card
            row gap:20 align:center
                img src:"dot.png" width:64 height:64
                img src:"dot.png" raster:nearest width:96 height:96
        code_line code:"img src:'assets/dot.png'   (decoded + baked, no runtime loader)"
    example title:"Attributes"
        col gap:6
            prop_row name:"src" values:"Arc<ImageData> · 'path'" about:"Runtime bitmap, or a baked file path."
            prop_row name:"filter" values:"Linear · Nearest" about:"Sampling when scaled (default Linear)."
            prop_row name:"fit" values:"contain · cover · fill" about:"Aspect handling in the box (default contain)."
