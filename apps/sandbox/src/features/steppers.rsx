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
    doc_header kicker:"INTERACTION" title:"Stepper" desc:"stepper is one numeric control: its − / + buttons, the arrow keys and a reader all step a bound value within its range, and the value reads back in your own range, no memo needed. Name it with a11y_label."
    example title:"stepper — bound value, clamped to its range, stepped by step"
        card gap:10
            row gap:16 align:center
                stepper value:$qty range:(0.0..=10.0) step:1 a11y_label:"Quantity"
                text "qty · {$qty}" font_size:14 color:$theme.muted
        code_line code:"stepper value:$qty range:(0.0..=10.0) step:1 a11y_label:\"Quantity\""
    example title:"on_change — a callback beside the binding, for the work a tap has to trigger"
        card gap:10
            row gap:16 align:center
                stepper value:$seats range:(1.0..=8.0) step:1 a11y_label:"Seats" on_change:(|v| $last_change.set(format!("stepped to {v:.0}")))
                text "{$last_change}" font_size:14 color:$theme.muted
        code_line code:"stepper value:$seats on_change:|v| $last_change.set(…)   (the binding still updates on its own)"
