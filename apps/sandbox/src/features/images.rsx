[logic]
use crate::shared::demo_images::{make_checker, make_gradient, make_radial_alpha};
use std::sync::Arc;

// Bitmaps generated pixel-by-pixel at startup — see demo_images.rs. Any Arc<ImageData> works as a src.
let gradient = Arc::new(make_gradient(128, 128));
let checker = Arc::new(make_checker(64, 64, 8));
let alpha = Arc::new(make_radial_alpha(128, 128));

[view]
col gap:20
    doc_header kicker:"09 · MEDIA" title:"Images" desc:"img draws an RGBA bitmap. Feed it an Arc<ImageData> built in Rust, or a quoted path that is decoded and baked into the binary at build time."
    col gap:8
        text "A procedural bitmap built from raw pixels" size:13 color:ink
        card
            img src:gradient width:128 height:128
        code_line code:"let gradient = Arc::new(make_gradient(128, 128));   img src:gradient"
    col gap:8
        text "Scaling filter — Linear smooths, Nearest keeps hard pixels" size:13 color:ink
        card
            row gap:20 align:end wrap
                col gap:6 align:center
                    img src:checker filter:Linear width:120 height:120
                    text "Linear" size:12 color:muted
                col gap:6 align:center
                    img src:checker filter:Nearest width:120 height:120
                    text "Nearest" size:12 color:muted
        code_line code:"img src:checker filter:Nearest   (a 64px bitmap upscaled to 120)"
    col gap:8
        text "object-fit — how the bitmap fills a non-square box" size:13 color:ink
        card
            row gap:20 wrap align:start
                col gap:6 align:center
                    img src:gradient fit:contain width:150 height:80
                    text "contain (default)" size:12 color:muted
                col gap:6 align:center
                    img src:gradient fit:cover width:150 height:80
                    text "cover" size:12 color:muted
                col gap:6 align:center
                    img src:gradient fit:fill width:150 height:80
                    text "fill" size:12 color:muted
        code_line code:"img src:gradient fit:cover width:150 height:80"
    col gap:8
        text "A PNG baked from disk at build time" size:13 color:ink
        card
            row gap:20 align:center
                img src:"dot.png" width:64 height:64
                img src:"dot.png" filter:Nearest width:96 height:96
        code_line code:"img src:'assets/dot.png'   (decoded + baked, no runtime loader)"
    col gap:8
        text "Attributes" size:13 color:ink
        col gap:6
            prop_row name:"src" values:"Arc<ImageData> · 'path'" about:"Runtime bitmap, or a baked file path."
            prop_row name:"filter" values:"Linear · Nearest" about:"Sampling when scaled (default Linear)."
            prop_row name:"fit" values:"contain · cover · fill" about:"Aspect handling in the box (default contain)."
