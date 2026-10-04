[logic]
use crate::shared::components::card::{card, CardProps};
use crate::shared::components::code_line::{code_line, CodeLineProps};
use crate::shared::components::doc_header::{doc_header, DocHeaderProps};
use crate::shared::components::example::{example, ExampleProps};
use crate::shared::components::prop_row::{prop_row, PropRowProps};

let presses = signal(0i32);
let calm = signal(false);
let bold = signal(true);
let italic = signal(false);
let calm_reading = memo(move || if calm.get() { "on" } else { "off" }.to_string());
let styles = memo(move || match (bold.get(), italic.get()) {
    (true, true) => "bold and italic",
    (true, false) => "bold",
    (false, true) => "italic",
    (false, false) => "plain",
}.to_string());

[view]
col gap:20
    doc_header kicker:"INTERACTION" title:"Controls with state" desc:"A role a person operates makes a pressable box a control: Tab stops on it, Enter and Space press it the way a tap does, and the theme's ring shows where the keyboard is. toggled: gives it an on/off state, which its role says how to announce: a switch is checked, a toggle button pressed."
    example title:"role:button — a box the keyboard reaches and presses"
        card
            row gap:12 align:center
                box role:button label:"Add one" fill:$theme.surface_alt stroke:$theme.border radius:8 pad_x:14 pad_y:8 cursor:pointer on_press:(|| $presses += 1)
                    text "+1" font_size:13 color:$theme.ink
                text "pressed {$presses} times: Tab to it, then Enter or Space" font_size:12 color:$theme.muted
        code_line code:"box role:button label:'Add one' on_press:(|| $presses += 1)"
    example title:"role:switch toggled: — on or off, read as checked"
        card
            row gap:12 align:center
                box role:switch toggled:$calm label:"Reduce motion" fill:(if $calm { $theme.primary } else { $theme.surface_alt }) stroke:$theme.border radius:14 pad_x:14 pad_y:6 cursor:pointer on_press:(|| $calm.set(!$calm.get()))
                    text "≈ {$calm_reading}" font_size:13 color:(if $calm { $theme.on_primary } else { $theme.ink })
                text "a reader hears “Reduce motion, switch, {$calm_reading}”" font_size:12 color:$theme.muted
        code_line code:"box role:switch toggled:$calm label:'Reduce motion' on_press:(|| $calm.set(!$calm.get()))"
    example title:"role:button toggled: — a toggle button, read as pressed"
        card
            row gap:8 align:center
                box role:button toggled:$bold label:"Bold" fill:(if $bold { $theme.primary } else { $theme.surface_alt }) stroke:$theme.border radius:6 width:32 height:32 align:center justify:center cursor:pointer on_press:(|| $bold.set(!$bold.get()))
                    text "B" font_size:14 font_weight:bold color:(if $bold { $theme.on_primary } else { $theme.ink })
                box role:button toggled:$italic label:"Italic" fill:(if $italic { $theme.primary } else { $theme.surface_alt }) stroke:$theme.border radius:6 width:32 height:32 align:center justify:center cursor:pointer on_press:(|| $italic.set(!$italic.get()))
                    text "I" font_size:14 color:(if $italic { $theme.on_primary } else { $theme.ink })
                text "the selection is {$styles}" font_size:12 color:$theme.muted
        code_line code:"box role:button toggled:$bold label:'Bold' on_press:(|| $bold.set(!$bold.get()))"
    example title:"Attributes"
        col gap:6
            prop_row name:"role" values:"button · switch · checkbox · radio · tab · slider · link · …" about:"A role a person operates makes the box a control: a Tab stop with the theme's focus ring, pressed by Enter and Space (Enter alone for a link). Regions and other roles only describe the box."
            prop_row name:"toggled" values:"$bool · expression reading $state" about:"The on/off state: checked on switch, checkbox and radio, pressed on button, selected on tab. Re-read when what it reads changes; a role with no such state is a build error."
