[logic]
pub struct Props {
    pub icon: std::sync::Arc<SvgData>,
    pub logo: std::sync::Arc<SvgData>,
    pub blurred: std::sync::Arc<SvgData>,
}

[view]
col gap:8
    text "Svg" size:12 color:muted
    col gap:4
        text "scales" size:11 color:muted
        row gap:20 align:end
            svg src:props.icon width:16 height:16
            svg src:props.icon width:32 height:32
            svg src:props.icon width:64 height:64
            svg src:props.icon width:128 height:128
    col gap:4
        text "tint" size:11 color:muted
        row gap:20 align:center
            svg src:props.icon tint:Color::from_hex("#e63946").unwrap() width:48 height:48
            svg src:props.icon tint:Color::from_hex("#2a9d8f").unwrap() width:48 height:48
            svg src:props.icon tint:Color::from_hex("#457b9d").unwrap() width:48 height:48
            svg src:props.icon tint:Color::from_hex("#f4a261").unwrap() width:48 height:48
    row gap:20
        col gap:4
            svg src:props.logo
            text "gradient logo" size:11 color:muted
        col gap:4
            svg src:props.blurred width:96 height:96
            text "blur (raster fallback)" size:11 color:muted

[preview "Svg gallery"]
sections_svg_section icon:crate::demo_svgs::make_icon() logo:crate::demo_svgs::make_logo() blurred:crate::demo_svgs::make_blurred()
