[logic]
let clicks = signal(0i32);

[view]
col gap:20
    doc_header kicker:"13 · INTERACTION" title:"Buttons" desc:"btn takes a label, a variant, and an on_press closure. The variant is the color token you pass to fill or outline; ghost is a bare flag."

    col gap:8
        text "Variants" size:13 color:ink
        box fill:surface stroke:border radius:12 pad:16
            row gap:12 wrap align:center
                btn "Filled" fill:primary
                btn "Outline" outline:primary
                btn "Ghost" ghost
                btn "Danger" fill:danger
                btn "Success" fill:success
        code_line code:"btn 'Filled' fill:primary   ·   outline:primary   ·   ghost"

    col gap:8
        text "on_press — every click runs a closure that mutates a signal" size:13 color:ink
        box fill:surface stroke:border radius:12 pad:16 gap:12
            text "Clicks · {$clicks}" size:18 color:ink
            row gap:10
                btn "+1" fill:primary on_press:|| $clicks.update(|n| *n += 1)
                btn "+10" fill:primary on_press:|| $clicks.update(|n| *n += 10)
                btn "Reset" ghost on_press:|| $clicks.set(0)
        code_line code:"btn '+1' fill:primary on_press:|| $clicks.update(|n| *n += 1)"

    col gap:8
        text "Attributes" size:13 color:ink
        col gap:6
            prop_row name:"fill" values:"token" about:"Filled button; label is drawn in white."
            prop_row name:"outline" values:"token" about:"Outlined button that fills on hover."
            prop_row name:"ghost" values:"flag" about:"Transparent button with neutral text."
            prop_row name:"on_press" values:"closure" about:"Runs on click; use $signal to read or mutate state."
