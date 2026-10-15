[logic]
use crate::shared::components::card::{CardProps, card};
use crate::shared::components::code_line::{CodeLineProps, code_line};
use crate::shared::components::doc_header::{DocHeaderProps, doc_header};
use crate::shared::components::example::{ExampleProps, example};
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
