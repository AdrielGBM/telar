[view]
col gap:20
    doc_header kicker:"OVERLAYS" title:"Dialogs" desc:"Modal, drawer and tooltip — high-level widgets built on the overlay primitive (a top-layer portal that escapes clipping and blocks the page). Each renders its content only while open, so a closed overlay costs nothing and never eats a background click."
    example title:"Modal — an opaque dialog centred over a dimming scrim; the scrim or Close dismisses"
        card gap:10
            text "Naming the dialog is what removes the signal(bool) per overlay: anything can open it by name — a menu item, a shortcut, a deep link — without the state being threaded down to it." size:13 color:muted
            button label:"Open modal" fill:primary on_press(|| open_overlay("confirm"))
            modal id:"confirm" title:"Confirm"
                text "Body content here" size:14 color:ink
        code_line code:"modal id:\"confirm\"  >  …      open_overlay(\"confirm\")  from anywhere"
    example title:"Drawer — a full-height side panel pinned to an edge, over the same scrim"
        card gap:10
            button label:"Open drawer" fill:primary on_press(|| open_overlay("nav"))
            drawer id:"nav" side:"right"
                text "Drawer content" color:ink
        code_line code:"drawer id:\"nav\" side:\"right\"  >  text \"Drawer content\""
    example title:"Tooltip — a hover popup anchored just below its trigger"
        card gap:10
            tooltip text:"Helpful hint"
                button label:"Hover me"
        code_line code:"tooltip text:\"Helpful hint\"  >  button label:\"Hover me\""
