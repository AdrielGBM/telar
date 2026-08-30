[logic]
#[derive(Default)]
pub struct Props {
    pub icon: &'static str,
    pub title: &'static str,
    pub body: &'static str,
}

[view]
box fill:$theme.surface stroke:$theme.border radius:16 width:300 min_width:260 grow:1 pad:24 gap:10 axis:col
    text "{props.icon}" font_size:32
    text "{props.title}" font_size:18 color:$theme.dark
    text "{props.body}" font_size:14 color:$theme.muted

[preview "Fast"]
feature_card icon:"⚡" title:"Fast" body:"Software and wgpu renderers with dirty-tracking and scroll-blit detection."

[preview "Long body"]
feature_card icon:"📱" title:"Cross-platform" body:"One codebase targets desktop and Android with native event loops, plus a longer body to test how the card wraps multi-line text."
