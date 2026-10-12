[logic]
/// A feature highlighted on the landing page.
#[derive(Default)]
pub struct Props {
    /// Short heading, one line.
    pub title: &'static str,
    /// Supporting sentence; wraps when long.
    pub body: &'static str,
}

[view]
box fill:$theme.surface stroke:$theme.border radius:16 width:300 min_width:260 grow:1 pad:24 gap:10 axis:col
    children name:"icon"
    text "{props.title}" font_size:18 color:$theme.dark
    text "{props.body}" font_size:14 color:$theme.muted

[previews "Landing/Feature card" layout:centered]
A feature highlighted on the landing page: an icon in the `icon` slot, a one-line heading and a supporting sentence that wraps.

[preview "Fast"]
feature_card title:"Fast" body:"Software and wgpu renderers with dirty-tracking and scroll-blit detection."
    icon name:"lucide:zap" size:32 color:$theme.accent slot:"icon"

[preview "Long body" viewport:360x640]
feature_card title:"Cross-platform" body:"One codebase targets desktop and Android with native event loops, plus a longer body to test how the card wraps multi-line text."
    icon name:"lucide:smartphone" size:32 color:$theme.accent slot:"icon"
