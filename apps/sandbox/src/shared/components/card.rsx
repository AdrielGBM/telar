[logic]
// The standard surface panel used across the sandbox. The card recipe (fill:theme.surface stroke:theme.border
// radius:12 pad:16) lives here once instead of inline at every call site. `gap` spaces the stacked
// body children (pass 0 for a single child). `pad` overrides the 16px inset via the `= 16.0` inline
// default, so an omitted `pad` is 16 while `pad:0` genuinely means zero. An optional "header" slot renders above the body.
#[derive(Default)]
pub struct Props {
    pub gap: f32,
    pub pad: f32 = 16.0,
}

[view]
box fill:theme.surface stroke:theme.border radius:12 pad:props.pad gap:props.gap
    children name:"header"
    children

[preview "Card"]
card gap:8
    text "Header" font_size:16 color:theme.ink slot:"header"
    text "A card is the standard surface panel." font_size:13 color:theme.muted
    text "Bare children stack with the gap you pass; slot:\"header\" pins a header on top." font_size:13 color:theme.muted
