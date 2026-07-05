[logic]
use crate::core::theme::theme;

let accent = signal(theme().primary);
let alt = signal(false);
let fade = signal(1.0f32);

[view]
col gap:20
    doc_header kicker:"15 · INTERACTION" title:"Transitions" desc:"Add transition: to any animatable property and a value change eases over time instead of snapping. Choose a duration + easing, or a spring."

    col gap:8
        text "Color — the fill eases to its new value" size:13 color:ink
        box fill:surface stroke:border radius:12 pad:16 gap:12
            row gap:14 align:center wrap
                col gap:6 align:center
                    box width:72 height:72 fill:$accent radius:14 transition:fill 250ms ease-out
                    text "250ms ease-out" size:11 color:muted
                col gap:6 align:center
                    box width:72 height:72 fill:$accent radius:14 transition:fill spring(170, 16)
                    text "spring(170, 16)" size:11 color:muted
                btn "Toggle" fill:primary on_press:|| { let a = $alt.peek(); $alt.set(!a); $accent.set(if a { theme().primary } else { theme().purple }) }
        code_line code:"box fill:$accent transition:fill 250ms ease-out"

    col gap:8
        text "Opacity — the same signal, animated" size:13 color:ink
        box fill:surface stroke:border radius:12 pad:16
            row gap:14 align:center wrap
                box width:130 height:64 fill:primary radius:12 opacity:$fade transition:opacity 300ms ease-in-out align:center justify:center
                    text "fade" size:13 color:on_primary
                btn "Toggle" fill:primary on_press:|| { let v = $fade.peek(); $fade.set(if v > 0.5 { 0.15 } else { 1.0 }) }
        code_line code:"box opacity:$fade transition:opacity 300ms ease-in-out"

    col gap:8
        text "Notes" size:13 color:ink
        col gap:6
            prop_row name:"transition" values:"prop dur easing" about:"e.g. fill 250ms ease-out — runs when the value changes."
            prop_row name:"properties" values:"fill·stroke·color·opacity" about:"Only these animate today."
            prop_row name:"spring(k, c)" values:"stiffness, damping" about:"Physics curve instead of a fixed duration."
