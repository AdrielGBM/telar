[logic]
use crate::shared::components::card::{CardProps, card};
use crate::shared::components::code_line::{CodeLineProps, code_line};
use crate::shared::components::doc_header::{DocHeaderProps, doc_header};
use crate::shared::components::example::{ExampleProps, example};
let volume = signal(40.0f32);
let temp = signal(65.0f32);

[view]
col gap:20
    doc_header kicker:"INTERACTION" title:"Slider" desc:"A drag-driven control built on the on_drag primitive — track, fill, and thumb are wired up for you. Set its range to read the value back in your own range (no memo needed), step to quantize it, and label to caption it."
    example title:"range — the value reads back in your range, not 0..1"
        card gap:8
            slider value:volume range:(0.0..=100.0) step:1 width:260 a11y_label:"Volume"
            text "Volume · {$volume}%" font_size:14 color:$theme.muted
        code_line code:"slider value:volume range:(0.0..=100.0) step:1 width:260 a11y_label:'Volume'"
    example title:"label + step — a captioned, quantized range"
        card gap:8
            slider value:temp range:(60.0..=80.0) step:5 label:"Temperature" width:260
            text "{$temp}°F" font_size:14 color:$theme.muted
        code_line code:"slider value:temp range:(60.0..=80.0) step:5 label:'Temperature'"
