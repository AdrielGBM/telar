[logic]
use crate::shared::components::card::{CardProps, card};
use crate::shared::components::code_line::{CodeLineProps, code_line};
use crate::shared::components::doc_header::{DocHeaderProps, doc_header};
use crate::shared::components::example::{ExampleProps, example};
// A bound bool for the checkbox, another for the toggle, and an optional index for the radio group's selection.
let agree = signal(false);
let notify = signal(true);
let choice = signal(Some(0usize));

[view]
col gap:20
    doc_header kicker:"INTERACTION" title:"Form controls" desc:"checkbox, toggle, and radio_group are components (from the components feature, not base tags). Each binds a signal two-way: the widget renders from it and a tap or a key writes it back. A radio group is one Tab stop; the arrows move between its options and choose them."
    example title:"checkbox — a bound bool; tap the box or its label to toggle"
        card gap:10
            checkbox checked:agree label:"I agree to the terms"
            text "agree · {$agree}" font_size:14 color:$theme.muted
        code_line code:"checkbox checked:agree label:'I agree to the terms'"
    example title:"toggle — a switch over the same kind of bool signal"
        card gap:10
            toggle checked:notify label:"Email notifications"
            text "notify · {$notify}" font_size:14 color:$theme.muted
        code_line code:"toggle checked:notify label:'Email notifications'"
    example title:"radio_group — one signal holding the chosen index, or none"
        card gap:10
            radio_group selected:choice items:vec!["Small","Medium","Large"] label:"Size"
            text "choice · {$choice.map_or(String::new(), |i| i.to_string())}" font_size:14 color:$theme.muted
        code_line code:"radio_group selected:choice items:vec!['Small','Medium','Large'] label:'Size'"
