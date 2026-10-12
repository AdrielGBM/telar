[logic]
#[derive(Default)]
pub struct Props {
    pub title: &'static str,
    pub body: &'static str,
}

[view]
box fill:$theme.surface stroke:$theme.border radius:16 grow:1 min_width:170 pad:20 gap:8
    children name:"icon"
    text "{props.title}" font_size:16 color:$theme.ink
    text "{props.body}" font_size:13 color:$theme.muted

[previews "Sandbox/Feature card" layout:centered]
An icon, title and body on a bordered surface, for the feature grid.

[preview "Feature card" matrix:(mode:[modern midnight])]
feature_card title:"Fast" body:"Software and wgpu renderers with dirty-tracking and scroll-blit detection."
    icon name:"lucide:zap" size:28 color:$theme.primary slot:"icon"
