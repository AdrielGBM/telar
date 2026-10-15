[logic]
use telar::preview::ActionCall;
use telar_devtools::{WORKBENCH_GRID, use_workbench_tokens};

/// One call the log took: its number, the callback's name and what it was called with.
pub struct Props {
    pub call: ActionCall,
}

let seq = format!("#{}", props.call.seq);
let name = props.call.name;
let payload = props.call.args.join(", ");

[style]
@line
    align: center
    gap: WORKBENCH_GRID * 2.0
    padding_x: WORKBENCH_GRID * 1.5
    padding_y: WORKBENCH_GRID
    stroke: use_workbench_tokens().border_subtle
    stroke_bottom: 1

@mono
    font_family: use_workbench_tokens().mono_family
    font_size: use_workbench_tokens().mono_size

[view]
row @line
    text @mono "{seq}" width:48 shrink:0 color:$theme.muted
    text "{name}" width:160 shrink:0 font_weight:semibold
    text @mono "{payload}" grow:1 basis:0 min_width:0
