[view]
section "Static assets (baked)"
    text "svg + img with a quoted src are resolved from disk and baked to native draw data at build time — no usvg at runtime" size:11 color:muted
    col gap:4
        text "baked vector svg (paths + gradient + stroke)" size:11 color:muted
        row gap:20 align:end
            svg src:"assets/badge.svg" width:24 height:24
            svg src:"assets/badge.svg" width:48 height:48
            svg src:"assets/badge.svg" width:96 height:96
    col gap:4
        text "baked vector svg, tinted" size:11 color:muted
        row gap:20 align:center
            svg src:"assets/badge.svg" tint:Color::from_hex("#e63946").unwrap() width:48 height:48
            svg src:"assets/badge.svg" tint:Color::from_hex("#2a9d8f").unwrap() width:48 height:48
    col gap:4
        text "baked raster svg (feGaussianBlur → raster fallback)" size:11 color:muted
        row gap:20 align:center
            svg src:"assets/glow.svg" width:48 height:48
            svg src:"assets/glow.svg" width:96 height:96
    col gap:4
        text "baked png image" size:11 color:muted
        row gap:20 align:center
            img src:"assets/dot.png" width:48 height:48
            img src:"assets/dot.png" filter:Nearest width:96 height:96
    col gap:4
        text "object-fit — svg in a non-square 120x60 box (fill distorts the aspect ratio)" size:11 color:muted
        row gap:20 align:start
            col gap:4 align:center
                svg src:"assets/badge.svg" fit:contain width:120 height:60
                text "contain (default)" size:11 color:muted
            col gap:4 align:center
                svg src:"assets/badge.svg" fit:cover width:120 height:60
                text "cover" size:11 color:muted
            col gap:4 align:center
                svg src:"assets/badge.svg" fit:fill width:120 height:60
                text "fill" size:11 color:muted
    col gap:4
        text "object-fit — img in a non-square 120x60 box" size:11 color:muted
        row gap:20 align:start
            col gap:4 align:center
                img src:"assets/dot.png" fit:contain width:120 height:60
                text "contain (default)" size:11 color:muted
            col gap:4 align:center
                img src:"assets/dot.png" fit:cover width:120 height:60
                text "cover" size:11 color:muted
            col gap:4 align:center
                img src:"assets/dot.png" fit:fill width:120 height:60
                text "fill" size:11 color:muted

[preview "Static assets"]
sections_static_assets_section
