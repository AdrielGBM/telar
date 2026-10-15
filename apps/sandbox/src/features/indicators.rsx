[logic]
use crate::shared::components::card::{CardProps, card};
use crate::shared::components::code_line::{CodeLineProps, code_line};
use crate::shared::components::doc_header::{DocHeaderProps, doc_header};
use crate::shared::components::example::{ExampleProps, example};
let p = signal(0.35f32);
let pct = memo(move || (p.get() * 100.0) as i32);

[view]
col gap:20
    doc_header kicker:"FEEDBACK" title:"Progress & spinner" desc:"progress is a determinate bar you drive with a 0..1 signal; spinner is an indeterminate ring that animates itself. Both are components (from the components feature, not base tags)."
    example title:"progress — a determinate bar bound to a 0..1 signal"
        card gap:10
            progress value:$p width:280
            row gap:12 align:center
                text "Loading · {$pct}%" font_size:14 color:$theme.muted
                button label:"Advance" color:$theme.primary on_press:(|| { $p.set(($p.get() + 0.15).min(1.0)) })
                button label:"Reset" variant:ButtonVariant::Ghost on_press:(|| { $p.set(0.0) })
        code_line code:"progress value:$p width:280"
    example title:"spinner — an indeterminate ring; it drives its own rotation"
        card gap:10
            row gap:16 align:center
                spinner size:28
                spinner size:20 color:$theme.success
                text "working…" font_size:14 color:$theme.muted
        code_line code:"spinner size:28"
