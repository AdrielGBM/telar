[props]
gradient: std::sync::Arc<ImageData>
checker: std::sync::Arc<ImageData>
alpha: std::sync::Arc<ImageData>

[view]
col gap:8
    text "Images" size:12 color:muted
    row gap:20
        col gap:4
            img src:gradient filter:Linear width:128 height:128
            text "gradient" size:11 color:muted width:128
        col gap:4
            img src:checker filter:Nearest width:192 height:192
            text "checker (scaled)" size:11 color:muted width:192
        col gap:4
            img src:alpha filter:Nearest width:128 height:128
            text "alpha blend" size:11 color:muted width:128
