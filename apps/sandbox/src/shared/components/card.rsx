[logic]
// The standard surface panel used across the sandbox. The card recipe (fill:surface stroke:border
// radius:12 pad:16) lives here once instead of inline at every call site. `gap` spaces the stacked
// body children (pass 0 for a single child). `pad` overrides the 16px inset (0 keeps the default, since
// derive(Default) can't express a non-zero one). An optional "header" slot renders above the body.
#[derive(Default)]
pub struct Props {
    pub gap: f32,
    pub pad: f32,
}
let pad = if props.pad > 0.0 { props.pad } else { 16.0 };

[view]
box fill:surface stroke:border radius:12 pad:pad gap:props.gap
    children name:"header"
    children

[preview "Card"]
card gap:8
    text "Header" size:16 color:ink slot:"header"
    text "A card is the standard surface panel." size:13 color:muted
    text "Bare children stack with the gap you pass; slot:\"header\" pins a header on top." size:13 color:muted
