[logic]
use crate::shared::components::card::{card, CardProps};
use crate::shared::components::code_line::{code_line, CodeLineProps};
use crate::shared::components::doc_header::{doc_header, DocHeaderProps};
use crate::shared::components::example::{example, ExampleProps};

let narrow = signal(false);
let share = memo(move || FitWidth::Containing(if narrow.get() { 0.4 } else { 0.9 }));

[view]
col gap:20
    doc_header kicker:"FOUNDATIONS" title:"Fitted text" desc:"font_size:fit(…) asks for a width instead of a size. Layout measures the line at two sizes through whatever measures text on this target and solves for the size that sets it to that width, again whenever the width, the text, its style or its face changes. The line scales whole, tracking in em included, and never wraps."
    example title:"A share of the box holding the text"
        card gap:8
            text "TELAR" font_size:fit(60%) font_weight:800 letter_spacing:-0.04em color:$theme.ink
            text "WOVEN ON EVERY TARGET" font_size:fit(60%) font_weight:800 letter_spacing:0.04em color:$theme.muted
        code_line code:"text 'TELAR' font_size:fit(60%) letter_spacing:-0.04em"
    example title:"Capped by a height"
        card
            text "Wide" font_size:fit(100%, max_height:18sh) font_weight:800 color:$theme.ink
        code_line code:"text 'Wide' font_size:fit(100%, max_height:18sh)"
    example title:"A width that follows state"
        card gap:8
            text "Reflow" font_size:fit($share) font_weight:600 color:$theme.primary
            button label:"Switch between 90% and 40%" ghost on_press:(|| $narrow.toggle())
        code_line code:"let share = memo(move || FitWidth::Containing(…));   text font_size:fit($share)"
    example title:"A length, and the face the app ships"
        card gap:8
            text "320 pixels wide" font_size:fit(320) color:$theme.ink
            text "Telar Test" font_size:fit(80%) font_family:"Telar Test, sans_serif" color:$theme.ink
            text "Fitted in the fallback first, then again on the frame the face lands." font_size:13 color:$theme.muted
        code_line code:"text font_size:fit(320)   ·   fit(40sw)   ·   fit(12em)"
