[logic]
use crate::shared::components::card::{card, CardProps};
use crate::shared::components::code_line::{code_line, CodeLineProps};
use crate::shared::components::doc_header::{doc_header, DocHeaderProps};
use crate::shared::components::example::{example, ExampleProps};
use crate::shared::components::prop_row::{prop_row, PropRowProps};

let accent = signal(theme.get().primary);
let alt = signal(false);
let fade = signal(1.0f32);

[view]
col gap:20
    doc_header kicker:"INTERACTION" title:"Transitions" desc:"Add transition(…) to any animatable property and a value change eases over time instead of snapping. Choose a duration + easing, or a spring."
    example title:"Color — the fill eases to its new value"
        card gap:12
            row gap:14 align:center wrap
                col gap:6 align:center
                    box width:72 height:72 fill:$accent radius:14 transition(fill 250ms ease-out)
                    text "250ms ease-out" font_size:11 color:$theme.muted
                col gap:6 align:center
                    box width:72 height:72 fill:$accent radius:14 transition(fill spring(170, 16))
                    text "spring(170, 16)" font_size:11 color:$theme.muted
                button label:"Toggle" fill:$theme.primary on_press:(|| { $alt.toggle(); $accent.set(if $alt.get() { $theme.purple } else { $theme.primary }) })
        code_line code:"box fill:$accent transition(fill 250ms ease-out)"
    example title:"Opacity — the same signal, animated"
        card
            row gap:14 align:center wrap
                box width:130 height:64 fill:$theme.primary radius:12 opacity:$fade align:center justify:center transition(opacity 300ms ease-in-out)
                    text "fade" font_size:13 color:$theme.on_primary
                button label:"Toggle" fill:$theme.primary on_press:(|| { let v = $fade.peek(); $fade.set(if v > 0.5 { 0.15 } else { 1.0 }) })
        code_line code:"box opacity:$fade transition(opacity 300ms ease-in-out)"
    example title:"Notes"
        col gap:6
            prop_row name:"transition(…)" values:"prop dur easing" about:"e.g. fill 250ms ease-out — runs when the value changes."
            prop_row name:"properties" values:"fill·stroke·color·opacity" about:"Only these animate today."
            prop_row name:"spring(k, c)" values:"stiffness, damping" about:"Physics curve instead of a fixed duration."
