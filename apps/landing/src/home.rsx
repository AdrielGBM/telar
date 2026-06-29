[logic]
use std::sync::Arc;
use crate::demo_images::{make_gradient, make_checker, make_radial_alpha};

let signups = signal(0i32);
let spots_left = memo(move || (200 - signups.get()).max(0));
let gradient_img = Arc::new(make_gradient(640, 400));
let checker_img = Arc::new(make_checker(640, 400, 32));
let radial_img = Arc::new(make_radial_alpha(640, 400));

[style]
primary: #4361ee

@page
    direction: col

@navband
    direction: col
    align: center
    padding_x: 24
    padding_y: 14

@footband
    direction: col
    align: center
    padding_x: 24
    padding_y: 40

@band
    direction: col
    align: center
    padding_x: 24
    padding_y: 64

@wrap
    direction: col
    width: 100%
    max_width: 1120
    gap: 40

@navwrap
    direction: row
    width: 100%
    max_width: 1120
    align: center
    justify: between

@footwrap
    direction: row
    width: 100%
    max_width: 1120
    justify: between
    gap: 32

[view]
col @page
    box @navband fill:surface
        row @navwrap
            text "▲ rsx" size:20 color:dark
            row gap:24 align:center
                text "Features" size:14 color:muted
                text "Gallery" size:14 color:muted
                text "Pricing" size:14 color:muted
                btn "Get started" fill:primary on_press:|| $signups.update(|n| *n += 1)
    box @band fill:surface
        col @wrap
            row gap:48 wrap align:center
                col grow:1 min_width:320 gap:20
                    text "Native UIs in Rust, without the boilerplate" size:40 color:dark
                    text "rsx compiles declarative .rsx markup to GPU-accelerated widgets — signals, layout and theming included, from desktop to Android." size:18 color:muted
                    row gap:12 wrap
                        btn "Get started" fill:primary on_press:|| $signups.update(|n| *n += 1)
                        btn "Read the docs" outline:primary
                    text "{$spots_left} of 200 early-access seats left" size:13 color:accent
                col grow:1 min_width:320
                    img src:gradient_img width:100% height:320
    box @band fill:surface_alt
        col @wrap gap:24
            text "Trusted primitives" size:14 color:muted
            row gap:20 wrap
                box fill:surface stroke:border radius:14 grow:1 min_width:170 pad:24 gap:6 direction:col align:center
                    text "60 fps" size:30 color:primary
                    text "software + wgpu" size:13 color:muted
                box fill:surface stroke:border radius:14 grow:1 min_width:170 pad:24 gap:6 direction:col align:center
                    text "25" size:30 color:primary
                    text "modular crates" size:13 color:muted
                box fill:surface stroke:border radius:14 grow:1 min_width:170 pad:24 gap:6 direction:col align:center
                    text "2" size:30 color:primary
                    text "render backends" size:13 color:muted
                box fill:surface stroke:border radius:14 grow:1 min_width:170 pad:24 gap:6 direction:col align:center
                    text "0" size:30 color:primary
                    text "runtime GC pauses" size:13 color:muted
    box @band fill:surface
        col @wrap gap:28
            col gap:8
                text "Everything you need" size:28 color:dark
                text "Composable building blocks that scale from a button to a full app." size:16 color:muted
            row gap:24 wrap
                feature_card icon:"⚡" title:"Fast" body:"Software and wgpu renderers with dirty-tracking and scroll-blit detection."
                feature_card icon:"🧩" title:"Composable" body:"Signals, memos and reusable .rsx components compose right inside the markup."
                feature_card icon:"🎨" title:"Themeable" body:"Semantic color tokens resolve reactively, so dark mode is a single swap."
                feature_card icon:"📱" title:"Cross-platform" body:"One codebase targets desktop and Android with native event loops."
    box @band fill:surface_alt
        col @wrap gap:28
            col gap:8
                text "Built-in rendering" size:28 color:dark
                text "Gradients, images, paths and shadows — all GPU-accelerated." size:16 color:muted
            row gap:20 wrap
                col grow:1 min_width:280 gap:8
                    img src:gradient_img width:100% height:200
                    text "Linear gradients" size:13 color:muted
                col grow:1 min_width:280 gap:8
                    img src:checker_img width:100% height:200
                    text "Bitmap images" size:13 color:muted
                col grow:1 min_width:280 gap:8
                    img src:radial_img width:100% height:200
                    text "Radial alpha" size:13 color:muted
    box @band fill:surface
        col @wrap
            row gap:48 wrap align:center
                col grow:1 min_width:300
                    img src:checker_img width:100% height:280
                col grow:1 min_width:300 gap:14
                    text "Reactivity that stays out of your way" size:26 color:dark
                    text "Fine-grained signals update only the widgets that depend on them — no virtual DOM, no diffing, no re-render storms." size:16 color:muted
                    text "→ signal, memo, effect" size:14 color:primary
    box @band fill:primary
        col @wrap align:center gap:16
            text "Join the private beta" size:30 color:on_primary
            text "{$spots_left} of 200 seats remaining" size:16 color:on_primary
            row gap:12 align:center wrap
                btn "Reserve a seat" fill:accent on_press:|| $signups.update(|n| *n += 1)
                text "{$signups} developers reserved" size:14 color:on_primary
    box @footband fill:dark
        row @footwrap
            col gap:8 grow:1 min_width:200
                text "▲ rsx" size:18 color:on_primary
                text "Native UI framework for Rust." size:13 color:on_dark
            col gap:6 min_width:140
                text "Product" size:13 color:on_primary
                text "Features" size:13 color:on_dark
                text "Gallery" size:13 color:on_dark
            col gap:6 min_width:140
                text "Developers" size:13 color:on_primary
                text "Docs" size:13 color:on_dark
                text "Examples" size:13 color:on_dark

[preview "Landing — full page"]
home
