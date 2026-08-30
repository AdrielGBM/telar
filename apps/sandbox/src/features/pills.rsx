[logic]
use crate::shared::components::card::{card, CardProps};
use crate::shared::components::code_line::{code_line, CodeLineProps};
use crate::shared::components::doc_header::{doc_header, DocHeaderProps};
use crate::shared::components::example::{example, ExampleProps};
use crate::shared::components::prop_row::{prop_row, PropRowProps};
let tags = signal(3u32);

[view]
col gap:20
    doc_header kicker:"PRESENTATION" title:"Badges & chips" desc:"badge is a small solid status tag; chip is a softer outlined pill, optionally removable via on_close. Both are components."
    example title:"badge — a small solid accent tag"
        card gap:10
            row gap:8 align:center
                text "Inbox" font_size:15 color:$theme.ink
                badge label:"12"
                badge label:"NEW" color:$theme.success
                badge label:"BETA" color:$theme.purple
        code_line code:"badge label:'NEW' color:$theme.success"
    example title:"chip — a softer outlined pill; on_close makes it removable"
        card gap:10
            row gap:8 align:center
                chip label:"design"
                chip label:"rust"
                chip label:"removable" on_close:(|| { $tags.update(|n| if *n > 0 { *n -= 1 }) })
            text "chips · {$tags}" font_size:13 color:$theme.muted
        code_line code:"chip label:'removable' on_close:|| remove()"
    example title:"Attributes"
        col gap:6
            prop_row name:"label" values:"text" about:"the tag / pill text."
            prop_row name:"color" values:"token" about:"badge fill / chip accent (default: theme)."
            prop_row name:"on_close" values:"closure" about:"chip only; adds a × that fires this."
