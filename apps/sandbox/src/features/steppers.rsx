[logic]
use crate::shared::components::card::{CardProps, card};
use crate::shared::components::code_line::{CodeLineProps, code_line};
use crate::shared::components::doc_header::{DocHeaderProps, doc_header};
use crate::shared::components::example::{ExampleProps, example};
let qty = signal(1.0f32);
let seats = signal(2.0f32);
// Written by `on_change` rather than read from the bound signal, so the callback is what proves it fired.
let last_change = signal("no change yet".to_string());

[view]
col gap:20
    doc_header kicker:"INTERACTION" title:"Stepper" desc:"stepper is a numeric input with − / + buttons that step a bound value within min/max. A component built on the button primitive; the value reads back in your own range, no memo needed."
    example title:"stepper — bound value, clamped to min/max, stepped by step"
        card gap:10
            row gap:16 align:center
                stepper value:$qty min:0 max:10 step:1
                text "qty · {$qty}" font_size:14 color:$theme.muted
        code_line code:"stepper value:$qty min:0 max:10 step:1"
    example title:"on_change — a callback beside the binding, for the work a tap has to trigger"
        card gap:10
            row gap:16 align:center
                stepper value:$seats min:1 max:8 step:1 on_change:(|v| $last_change.set(format!("stepped to {v:.0}")))
                text "{$last_change}" font_size:14 color:$theme.muted
        code_line code:"stepper value:$seats on_change:|v| $last_change.set(…)   (the binding still updates on its own)"
