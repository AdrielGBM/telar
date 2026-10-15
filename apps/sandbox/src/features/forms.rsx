[logic]
use crate::shared::components::card::{CardProps, card};
use crate::shared::components::code_line::{CodeLineProps, code_line};
use crate::shared::components::doc_header::{DocHeaderProps, doc_header};
use crate::shared::components::example::{ExampleProps, example};
// A bound bool for the checkbox, another for the toggle, and a u32 for the radio group's selection.
let agree = signal(false);
let notify = signal(true);
let choice = signal(0u32);

[view]
col gap:20
    doc_header kicker:"INTERACTION" title:"Form controls" desc:"checkbox, toggle, and radio are components (from the components feature, not base tags). Each binds a signal two-way: the widget renders from it and a tap writes it back. A radio group is several radios sharing one signal, each with its own value."
    example title:"checkbox — a bound bool; tap the box or its label to toggle"
        card gap:10
            checkbox checked:$agree label:"I agree to the terms"
            text "agree · {$agree}" font_size:14 color:$theme.muted
        code_line code:"checkbox checked:$agree label:'I agree to the terms'"
    example title:"toggle — a switch over the same kind of bool signal"
        card gap:10
            toggle checked:$notify label:"Email notifications"
            text "notify · {$notify}" font_size:14 color:$theme.muted
        code_line code:"toggle checked:$notify label:'Email notifications'"
    example title:"radio — several buttons share one signal; each carries a distinct value"
        card gap:10
            radio selected:$choice value:0u32 label:"Small"
            radio selected:$choice value:1u32 label:"Medium"
            radio selected:$choice value:2u32 label:"Large"
            text "choice · {$choice}" font_size:14 color:$theme.muted
        code_line code:"radio selected:$choice value:0u32 label:'Small'      (a group shares one signal)"
