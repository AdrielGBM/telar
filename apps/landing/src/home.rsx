[logic]
use crate::feature_card::{feature_card, FeatureCardProps};
use crate::demo_images::{make_checker, make_gradient, make_radial_alpha};
use std::sync::Arc;

let signups = signal(0i32);
let spots_left = memo(move || (200 - signups.get()).max(0));
let gradient_img = Arc::new(make_gradient(640, 400));
let checker_img = Arc::new(make_checker(640, 400, 32));
let radial_img = Arc::new(make_radial_alpha(640, 400));

[style]
primary: #4361ee

@page
    axis: col

@navband
    axis: col
    align: center
    padding_x: 24
    padding_y: 14

@footband
    axis: col
    align: center
    padding_x: 24
    padding_y: 40

@band
    axis: col
    align: center
    padding_x: 24
    padding_y: 64

@wrap
    axis: col
    width: 100%
    max_width: 1120
    gap: 40

@navwrap
    axis: row
    width: 100%
    max_width: 1120
    align: center
    justify: between

@footwrap
    axis: row
    width: 100%
    max_width: 1120
    justify: between
    gap: 32

[view]
col @page
    box @navband fill:theme.surface
        row @navwrap
            text "▲ rsx" font_size:20 color:theme.dark
            row gap:24 align:center
                text "Features" font_size:14 color:theme.muted
                text "Gallery" font_size:14 color:theme.muted
                text "Pricing" font_size:14 color:theme.muted
                button label:"Get started" fill:primary on_press(|| $signups.update(|n| *n += 1))
    box @band fill:theme.surface
        col @wrap
            row gap:48 wrap align:center
                col grow:1 min_width:320 gap:20
                    text "Native UIs in Rust, without the boilerplate" font_size:40 color:theme.dark
                    text "rsx compiles declarative .rsx markup to GPU-accelerated widgets — signals, layout and theming included, from desktop to Android." font_size:18 color:theme.muted
                    row gap:12 wrap
                        button label:"Get started" fill:primary on_press(|| $signups.update(|n| *n += 1))
                        button label:"Read the docs" outline:primary
                    text "{$spots_left} of 200 early-access seats left" font_size:13 color:theme.accent
                col grow:1 min_width:320
                    img src:gradient_img width:100% height:320
    box @band fill:theme.surface_alt
        col @wrap gap:24
            text "Trusted primitives" font_size:14 color:theme.muted
            row gap:20 wrap
                box fill:theme.surface stroke:theme.border radius:14 grow:1 min_width:170 pad:24 gap:6 axis:col align:center
                    text "60 fps" font_size:30 color:primary
                    text "software + wgpu" font_size:13 color:theme.muted
                box fill:theme.surface stroke:theme.border radius:14 grow:1 min_width:170 pad:24 gap:6 axis:col align:center
                    text "25" font_size:30 color:primary
                    text "modular crates" font_size:13 color:theme.muted
                box fill:theme.surface stroke:theme.border radius:14 grow:1 min_width:170 pad:24 gap:6 axis:col align:center
                    text "2" font_size:30 color:primary
                    text "render backends" font_size:13 color:theme.muted
                box fill:theme.surface stroke:theme.border radius:14 grow:1 min_width:170 pad:24 gap:6 axis:col align:center
                    text "0" font_size:30 color:primary
                    text "runtime GC pauses" font_size:13 color:theme.muted
    box @band fill:theme.surface
        col @wrap gap:28
            col gap:8
                text "Everything you need" font_size:28 color:theme.dark
                text "Composable building blocks that scale from a button to a full app." font_size:16 color:theme.muted
            row gap:24 wrap
                feature_card icon:"⚡" title:"Fast" body:"Software and wgpu renderers with dirty-tracking and scroll-blit detection."
                feature_card icon:"🧩" title:"Composable" body:"Signals, memos and reusable .rsx components compose right inside the markup."
                feature_card icon:"🎨" title:"Themeable" body:"Semantic color tokens resolve reactively, so dark mode is a single swap."
                feature_card icon:"📱" title:"Cross-platform" body:"One codebase targets desktop and Android with native event loops."
    box @band fill:theme.surface_alt
        col @wrap gap:28
            col gap:8
                text "Built-in rendering" font_size:28 color:theme.dark
                text "Gradients, images, paths and shadows — all GPU-accelerated." font_size:16 color:theme.muted
            row gap:20 wrap
                col grow:1 min_width:280 gap:8
                    img src:gradient_img width:100% height:200
                    text "Linear gradients" font_size:13 color:theme.muted
                col grow:1 min_width:280 gap:8
                    img src:checker_img width:100% height:200
                    text "Bitmap images" font_size:13 color:theme.muted
                col grow:1 min_width:280 gap:8
                    img src:radial_img width:100% height:200
                    text "Radial alpha" font_size:13 color:theme.muted
    box @band fill:theme.surface
        col @wrap
            row gap:48 wrap align:center
                col grow:1 min_width:300
                    img src:checker_img width:100% height:280
                col grow:1 min_width:300 gap:14
                    text "Reactivity that stays out of your way" font_size:26 color:theme.dark
                    text "Fine-grained signals update only the widgets that depend on them — no virtual DOM, no diffing, no re-render storms." font_size:16 color:theme.muted
                    text "→ signal, memo, effect" font_size:14 color:primary
    box @band fill:primary
        col @wrap align:center gap:16
            text "Join the private beta" font_size:30 color:theme.on_primary
            text "{$spots_left} of 200 seats remaining" font_size:16 color:theme.on_primary
            row gap:12 align:center wrap
                button label:"Reserve a seat" fill:theme.accent on_press(|| $signups.update(|n| *n += 1))
                text "{$signups} developers reserved" font_size:14 color:theme.on_primary
    box @footband fill:theme.dark
        row @footwrap
            col gap:8 grow:1 min_width:200
                text "▲ rsx" font_size:18 color:theme.on_primary
                text "Native UI framework for Rust." font_size:13 color:theme.on_dark
            col gap:6 min_width:140
                text "Product" font_size:13 color:theme.on_primary
                text "Features" font_size:13 color:theme.on_dark
                text "Gallery" font_size:13 color:theme.on_dark
            col gap:6 min_width:140
                text "Developers" font_size:13 color:theme.on_primary
                text "Docs" font_size:13 color:theme.on_dark
                text "Examples" font_size:13 color:theme.on_dark

[preview "Landing — full page"]
home
