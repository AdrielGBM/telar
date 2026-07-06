[logic]
// The select binds a signal (the chosen index); the menu is stateless — each item is a one-shot action.
let picked = signal(0u32);
let action = signal(0u32);

[view]
col gap:20
    doc_header kicker:"15 · INTERACTION" title:"Menus & Selects" desc:"select and menu are components (from the components feature, not base tags) built on the overlay anchor: a trigger button opens a panel positioned next to it, and only that panel blocks clicks — taps elsewhere fall through and dismiss it."
    col gap:8
        text "select — a dropdown bound to a signal" size:13 color:ink
        card gap:10
            select selected:$picked options:vec!["Small","Medium","Large"]
            text "Size · {$picked}" size:14 color:muted
        code_line code:"select selected:$picked options:vec!['Small','Medium','Large']"
    col gap:8
        text "menu — a click-triggered list of one-shot actions" size:13 color:ink
        card gap:10
            menu label:"Actions" items:vec!["Rename","Duplicate","Delete"] on_select:|i| $action.set(i)
            text "Last action index · {$action}" size:14 color:muted
        code_line code:"menu label:'Actions' items:vec!['Rename','Duplicate','Delete'] on_select:|i| …"
    col gap:8
        text "Attributes" size:13 color:ink
        col gap:6
            prop_row name:"selected" values:"signal" about:"select: the bound choice index; omit for an uncontrolled select."
            prop_row name:"options / items" values:"vec![…]" about:"The labels listed in the panel, in order."
            prop_row name:"on_change / on_select" values:"closure" about:"Runs with the picked index when a choice is made."
            prop_row name:"color" values:"token" about:"Accent for the trigger border and highlight; falls back to the theme."
