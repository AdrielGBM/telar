[logic]
pub struct Props {
    pub gradient: std::sync::Arc<ImageData>,
    pub checker: std::sync::Arc<ImageData>,
    pub alpha: std::sync::Arc<ImageData>,
}

[view]
col gap:8
    text "Images" size:12 color:muted
    row gap:20
        col gap:4
            img src:props.gradient filter:Linear width:128 height:128
            text "gradient" size:11 color:muted width:128
        col gap:4
            img src:props.checker filter:Nearest width:192 height:192
            text "checker (scaled)" size:11 color:muted width:192
        col gap:4
            img src:props.alpha filter:Nearest width:128 height:128
            text "alpha blend" size:11 color:muted width:128
