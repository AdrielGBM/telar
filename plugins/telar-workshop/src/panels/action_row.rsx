[logic]
use telar::preview::ActionCall;

/// One call the log took: its number, the callback's name and what it was called with.
pub struct Props {
    pub call: ActionCall,
}

let seq = format!("#{}", props.call.seq);
let name = props.call.name;
let payload = props.call.args.join(", ");

[style]
use crate::styles::{line, mono};

[view]
row @line align:center
    text @mono "{seq}" width:48 shrink:0 color:$theme.muted
    text "{name}" width:160 shrink:0 font_weight:semibold
    text @mono "{payload}" grow:1 basis:0 min_width:0
