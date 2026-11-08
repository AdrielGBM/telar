[logic]
// `pad` defaults through `= 16.0`, so an omitted `pad` is 16 while `pad:0` means zero.
#[derive(Default)]
pub struct Props {
    pub gap: f32,
    pub pad: f32 = 16.0,
    #[props(default)]
    pub header: Children,
    #[props(default)]
    pub children: Children,
}

[view]
box fill:$theme.surface stroke:$theme.border radius:12 pad:props.pad gap:props.gap
    children $props.header
    children $props.children

[previews "Sandbox/Card" layout:centered]
The standard surface panel: a rounded, bordered container with an optional header slot above its stacked body.

[preview "Card" matrix:(mode:[modern midnight])]
card gap:8
    text "Header" font_size:16 color:$theme.ink slot:"header"
    text "A card is the standard surface panel." font_size:13 color:$theme.muted
    text "Bare children stack with the gap you pass; slot:\"header\" pins a header on top." font_size:13 color:$theme.muted
