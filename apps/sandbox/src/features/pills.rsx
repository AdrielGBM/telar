[logic]
use crate::shared::components::card::{CardProps, card};
use crate::shared::components::code_line::{CodeLineProps, code_line};
use crate::shared::components::doc_header::{DocHeaderProps, doc_header};
use crate::shared::components::example::{ExampleProps, example};
let tags = signal(3u32);
let rust = signal(true);

[view]
col gap:20
    doc_header kicker:"PRESENTATION" title:"Tags" desc:"tag is a short label: a status, a category, a value the user removes or a filter they select. What it does follows from the props it is handed, and how it looks is a variant and a tone. A component."
    example title:"tag — a status or a category, in a tone and a variant"
        card gap:10
            row gap:8 align:center
                text "Inbox" font_size:15 color:$theme.ink
                tag label:"12"
                tag label:"NEW" tone:success variant:soft
                tag label:"BETA" tone:$theme.purple variant:outline
        code_line code:"tag label:'NEW' tone:success"
    example title:"a control in the action slot acts on the tag; selected or on_toggle makes it a filter"
        card gap:10
            row gap:8 align:center
                tag label:"design"
                tag label:"rust" selected:$rust
                tag label:"removable"
                    icon_button icon:"lucide:x" label:"Remove removable" size:small on_press:(|| { $tags.update(|n| if *n > 0 { *n -= 1 }) }) slot:"action"
            text "tags · {$tags}" font_size:13 color:$theme.muted
        code_line code:"tag label:'removable' / icon_button icon:'lucide:x' size:small slot:'action'"
