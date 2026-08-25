[logic]
#[derive(Default)]
pub struct Props {
    pub icon: &'static str,
    pub title: &'static str,
    pub body: &'static str,
}

[view]
box fill:surface stroke:border radius:16 grow:1 min_width:170 pad:20 gap:8
    text "{props.icon}" font_size:28
    text "{props.title}" font_size:16 color:ink
    text "{props.body}" font_size:13 color:muted

[preview "Feature card"]
feature_card icon:"⚡" title:"Fast" body:"Software and wgpu renderers with dirty-tracking and scroll-blit detection."
