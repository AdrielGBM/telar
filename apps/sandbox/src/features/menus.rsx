[logic]
// The select binds a signal (the chosen index); the menu is stateless — each item is a one-shot action.
let picked = signal(0u32);
let action = signal(0u32);
// Driven from the toggles below, so the disabled row and the ticked one can be seen changing.
let cant_redo = signal(false);
let ruler = signal(true);

[view]
col gap:20
    doc_header kicker:"INTERACTION" title:"Menus & Selects" desc:"select and menu are components (from the components feature, not base tags) built on the overlay anchor: a trigger button opens a panel positioned next to it, and only that panel blocks clicks — taps elsewhere fall through and dismiss it."
    example title:"select — a dropdown bound to a signal"
        card gap:10
            select selected:$picked
                item label:"Small"
                item label:"Medium"
                item label:"Large"
            text "Size · {$picked}" size:14 color:muted
        code_line code:"select selected:$picked / item label:'Small' / item label:'Medium'"
    example title:"menu — a click-triggered list of one-shot actions"
        card gap:10
            menu label:"Actions" on_select(|i| $action.set(i))
                item label:"Rename"
                item label:"Duplicate"
            text "Last action index · {$action}" size:14 color:muted
        code_line code:"menu label:'Actions'   >   item label:'Rename'"
    example title:"A menu is a compound component — its rows are markup, not a list of strings"
        card gap:10
            menu label:"Edit" bordered
                item label:"Undo" hint:"⌘Z"
                item label:"Redo" hint:"⇧⌘Z" disabled:$cant_redo
                separator
                group label:"Clipboard"
                item label:"Cut" hint:"⌘X"
                item label:"Show ruler" checked:$ruler on_press(|| $ruler.set(!$ruler.get()))
            row gap:10 align:center
                toggle checked:$cant_redo label:"Disable Redo"
                toggle checked:$ruler label:"Ruler"
        code_line code:"item label:'Redo' disabled:$x   ·   separator   ·   group label:'…'   ·   item checked:$on"
    example title:"Attributes"
        col gap:6
            prop_row name:"selected" values:"signal" about:"select: the bound choice index; omit for an uncontrolled select."
            prop_row name:"options" values:"vec![…]" about:"select: the labels listed in the panel, in order."
            prop_row name:"item" values:"child" about:"menu: one row. Takes label, disabled, checked, hint, on_press — or markup children as its content."
            prop_row name:"separator / group" values:"child" about:"menu: a rule between groups, and a heading over one. The keyboard steps over both."
            prop_row name:"on_select" values:"closure" about:"Runs with the picked index when a choice is made."
            prop_row name:"color" values:"token" about:"Accent for the trigger border and highlight; falls back to the theme."
