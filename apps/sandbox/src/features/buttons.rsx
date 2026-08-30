[logic]
use crate::shared::components::card::{card, CardProps};
use crate::shared::components::code_line::{code_line, CodeLineProps};
use crate::shared::components::doc_header::{doc_header, DocHeaderProps};
use crate::shared::components::example::{example, ExampleProps};
use crate::shared::components::prop_row::{prop_row, PropRowProps};
let clicks = signal(0i32);
let hovering = signal(false);
let keys = signal(0i32);

[style]
@center
    align: center
    justify: center

[view]
col gap:20
    doc_header kicker:"INTERACTION" title:"Buttons" desc:"button is a component (from the components feature, not a base tag): it takes a label, a variant, and an on_press closure. The variant is the color token you pass to fill or outline; ghost is a bare flag."
    example title:"Variants"
        card
            row gap:12 wrap align:center
                button label:"Filled" fill:$theme.primary
                button label:"Outline" outline:$theme.primary
                button label:"Ghost" ghost
                button label:"Danger" fill:$theme.danger
                button label:"Success" fill:$theme.success
        code_line code:"button label:'Filled' fill:$theme.primary   ·   outline:$theme.primary   ·   ghost"
    example title:"on_press — every click runs a closure that mutates a signal"
        card gap:12
            text "Clicks · {$clicks}" font_size:18 color:$theme.ink
            row gap:10
                button label:"+1" fill:$theme.primary on_press:(|| $clicks += 1)
                button label:"+10" fill:$theme.primary on_press:(|| $clicks += 10)
                button label:"Reset" ghost on_press:(|| $clicks.set(0))
        code_line code:"button label:'+1' fill:$theme.primary on_press:|| $clicks += 1      ($x += n desugars to .update)"
    example title:"A whole box is clickable — on_press works on any container, not just buttons"
        card gap:10
            box @center fill:$theme.surface_alt radius:10 pad:20 on_press:(|| $clicks += 1)
                text "Tap anywhere in this card · {$clicks}" font_size:14 color:$theme.ink
            text "The card itself takes on_press; a child button would still win its own taps." font_size:12 color:$theme.muted
        code_line code:"box on_press(|| $clicks += 1)      (paren form: delimited, order-independent)"
    example title:"hover — a container restyles while the pointer is over it (mouse only)"
        card gap:10
            row gap:10 wrap
                box @center fill:$theme.surface_alt radius:10 pad:16 hover_style(fill:$theme.primary) width:150 height:64
                    text "Fill on hover" font_size:13 color:$theme.ink
                box @center fill:$theme.surface_alt stroke:$theme.border radius:10 pad:16 hover_style(stroke:$theme.primary) radius:16 width:150 height:64
                    text "Stroke + radius" font_size:13 color:$theme.ink
            text "Each box carries its own hover_style(...) — different hovers in one file, no signals." font_size:12 color:$theme.muted
        code_line code:"box fill:$theme.surface_alt hover_style(fill:$theme.primary)      (swap style while hovered)"
    example title:"Event callbacks — on_hover (a bool) and on_key (global shortcut)"
        card gap:10
            box fill:$theme.surface_alt radius:10 pad:16 on_hover:(|h| $hovering.set(h))
                text "hover me — hovering: {$hovering}" font_size:14 color:$theme.ink
            col on_key:(|_k| $keys += 1)
                text "keys pressed anywhere: {$keys}" font_size:14 color:$theme.muted
            text "on_hover fires with true/false; on_key has no per-widget focus, so it fires for every key." font_size:12 color:$theme.muted
        code_line code:"box on_hover(|h| $hovering.set(h))   ·   col on_key(|k| …)"
    example title:"Attributes"
        col gap:6
            prop_row name:"fill" values:"token" about:"Filled button; label is drawn in white."
            prop_row name:"outline" values:"token" about:"Outlined button that fills on hover."
            prop_row name:"ghost" values:"flag" about:"Transparent button with neutral text."
            prop_row name:"on_press" values:"closure" about:"Runs on click; use $signal to read or mutate state."
