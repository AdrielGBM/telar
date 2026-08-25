[logic]
#[derive(Default)]
pub struct Props {
    pub icon: &'static str,
    pub title: &'static str,
    pub body: &'static str,
}

[view]
box fill:surface stroke:border radius:16 width:300 min_width:260 grow:1 pad:24 gap:10 axis:col
    text "{props.icon}" size:32
    text "{props.title}" size:18 color:dark
    text "{props.body}" size:14 color:muted

[preview "Fast"]
feature_card icon:"⚡" title:"Fast" body:"Software and wgpu renderers with dirty-tracking and scroll-blit detection."

[preview "Long body"]
feature_card icon:"📱" title:"Cross-platform" body:"One codebase targets desktop and Android with native event loops, plus a longer body to test how the card wraps multi-line text."
