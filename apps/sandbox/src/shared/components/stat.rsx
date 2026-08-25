[logic]
#[derive(Default)]
pub struct Props {
    pub value: &'static str,
    pub label: &'static str,
}

[view]
box fill:surface stroke:border radius:14 grow:1 min_width:130 pad:18 gap:4 align:center
    text "{props.value}" font_size:28 color:primary
    text "{props.label}" font_size:12 color:muted

[preview "Stat"]
stat value:"60 fps" label:"software + wgpu"
