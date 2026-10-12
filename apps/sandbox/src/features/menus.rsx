[logic]
use crate::shared::components::card::{card, CardProps};
use crate::shared::components::code_line::{code_line, CodeLineProps};
use crate::shared::components::doc_header::{doc_header, DocHeaderProps};
use crate::shared::components::example::{example, ExampleProps};
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
            text "Size · {$picked}" font_size:14 color:$theme.muted
        code_line code:"select selected:$picked / item label:'Small' / item label:'Medium'"
    example title:"menu — a click-triggered list of one-shot actions"
        card gap:10
            menu label:"Actions" on_select:(|i| $action.set(i))
                item label:"Rename"
                item label:"Duplicate"
            text "Last action index · {$action}" font_size:14 color:$theme.muted
        code_line code:"menu label:'Actions'   >   item label:'Rename'"
    example title:"A menu is a compound component — its rows are markup, not a list of strings"
        card gap:10
            menu label:"Edit" bordered
                item label:"Undo" hint:"⌘Z"
                item label:"Redo" hint:"⇧⌘Z" disabled:$cant_redo
                separator
                group label:"Clipboard"
                item label:"Cut" hint:"⌘X"
                item label:"Show ruler" checked:$ruler on_press:(|| $ruler.set(!$ruler.get()))
            row gap:10 align:center
                toggle checked:$cant_redo label:"Disable Redo"
                toggle checked:$ruler label:"Ruler"
        code_line code:"item label:'Redo' disabled:$x   ·   separator   ·   group label:'…'   ·   item checked:$on"
