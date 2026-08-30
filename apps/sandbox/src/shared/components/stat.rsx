[logic]
#[derive(Default)]
pub struct Props {
    pub value: &'static str,
    pub label: &'static str,
}

[view]
box fill:$theme.surface stroke:$theme.border radius:14 grow:1 min_width:130 pad:18 gap:4 align:center
    text "{props.value}" font_size:28 color:$theme.primary
    text "{props.label}" font_size:12 color:$theme.muted

[preview "Stat"]
stat value:"60 fps" label:"software + wgpu"
