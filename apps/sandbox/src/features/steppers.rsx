[logic]
let qty = signal(1.0f32);

[view]
col gap:20
    doc_header kicker:"INTERACTION" title:"Stepper" desc:"stepper is a numeric input with − / + buttons that step a bound value within min/max. A component built on the button primitive; the value reads back in your own range, no memo needed."
    example title:"stepper — bound value, clamped to min/max, stepped by step"
        card gap:10
            row gap:16 align:center
                stepper value:$qty min:0 max:10 step:1
                text "qty · {$qty}" font_size:14 color:theme.muted
        code_line code:"stepper value:$qty min:0 max:10 step:1"
    example title:"Attributes"
        col gap:6
            prop_row name:"value" values:"signal" about:"the bound number, reactive."
            prop_row name:"min / max" values:"number" about:"clamp range; an unset max is unbounded."
            prop_row name:"step" values:"number" about:"increment per − / + tap (default 1)."
            prop_row name:"on_change" values:"closure" about:"fires with the new value on each tap."
