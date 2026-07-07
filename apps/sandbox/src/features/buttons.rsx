[logic]
let clicks = signal(0i32);
let hovering = signal(false);
let keys = signal(0i32);

[view]
col gap:20
    doc_header kicker:"13 · INTERACTION" title:"Buttons" desc:"button is a component (from the components feature, not a base tag): it takes a label, a variant, and an on_press closure. The variant is the color token you pass to fill or outline; ghost is a bare flag."
    col gap:8
        text "Variants" size:13 color:ink
        card
            row gap:12 wrap align:center
                button label:"Filled" fill:primary
                button label:"Outline" outline:primary
                button label:"Ghost" ghost
                button label:"Danger" fill:danger
                button label:"Success" fill:success
        code_line code:"button label:'Filled' fill:primary   ·   outline:primary   ·   ghost"
    col gap:8
        text "on_press — every click runs a closure that mutates a signal" size:13 color:ink
        card gap:12
            text "Clicks · {$clicks}" size:18 color:ink
            row gap:10
                button label:"+1" fill:primary on_press:|| $clicks += 1
                button label:"+10" fill:primary on_press:|| $clicks += 10
                button label:"Reset" ghost on_press:|| $clicks.set(0)
        code_line code:"button label:'+1' fill:primary on_press:|| $clicks += 1      ($x += n desugars to .update)"
    col gap:8
        text "A whole box is clickable — on_press works on any container, not just buttons" size:13 color:ink
        card gap:10
            box fill:surface_alt radius:10 pad:20 align:center justify:center on_press:|| $clicks += 1
                text "Tap anywhere in this card · {$clicks}" size:14 color:ink
            text "The card itself takes on_press; a child button would still win its own taps." size:12 color:muted
        code_line code:"box on_press(|| $clicks += 1)      (paren form: delimited, order-independent)"
    col gap:8
        text "hover — a container restyles while the pointer is over it (mouse only)" size:13 color:ink
        card gap:10
            row gap:10 wrap
                box fill:surface_alt radius:10 pad:16 hover_style:fill:primary align:center justify:center width:150 height:64
                    text "Fill on hover" size:13 color:ink
                box fill:surface_alt stroke:border radius:10 pad:16 hover_style:stroke:primary radius:16 align:center justify:center width:150 height:64
                    text "Stroke + radius" size:13 color:ink
            text "Each box carries its own hover_style(...) — different hovers in one file, no signals." size:12 color:muted
        code_line code:"box fill:surface_alt hover_style(fill:primary)      (swap style while hovered)"
    col gap:8
        text "Event callbacks — on_hover (a bool) and on_key (global shortcut)" size:13 color:ink
        card gap:10
            box fill:surface_alt radius:10 pad:16 on_hover:|h| $hovering.set(h)
                text "hover me — hovering: {$hovering}" size:14 color:ink
            col on_key:|_k| $keys += 1
                text "keys pressed anywhere: {$keys}" size:14 color:muted
            text "on_hover fires with true/false; on_key has no per-widget focus, so it fires for every key." size:12 color:muted
        code_line code:"box on_hover(|h| $hovering.set(h))   ·   col on_key(|k| …)"
    col gap:8
        text "Attributes" size:13 color:ink
        col gap:6
            prop_row name:"fill" values:"token" about:"Filled button; label is drawn in white."
            prop_row name:"outline" values:"token" about:"Outlined button that fills on hover."
            prop_row name:"ghost" values:"flag" about:"Transparent button with neutral text."
            prop_row name:"on_press" values:"closure" about:"Runs on click; use $signal to read or mutate state."
