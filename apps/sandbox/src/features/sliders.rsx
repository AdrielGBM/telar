[logic]
let volume = signal(40.0f32);
let temp = signal(65.0f32);

[view]
col gap:20
    doc_header kicker:"INTERACTION" title:"Slider" desc:"A drag-driven control built on the on_drag primitive — track, fill, and thumb are wired up for you. Set min/max to read the value back in your own range (no memo needed), step to quantize it, and label to caption it."
    example title:"min / max — the value reads back in your range, not 0..1"
        card gap:8
            slider value:$volume min:0 max:100 step:1 width:260
            text "Volume · {$volume}%" font_size:14 color:muted
        code_line code:"slider value:$volume min:0 max:100 step:1 width:260"
    example title:"label + step — a captioned, quantized range"
        card gap:8
            slider value:$temp min:60 max:80 step:5 label:"Temperature" width:260
            text "{$temp}°F" font_size:14 color:muted
        code_line code:"slider value:$temp min:60 max:80 step:5 label:'Temperature'"
    example title:"Attributes"
        col gap:6
            prop_row name:"value" values:"signal" about:"bound number; reads back in [min,max]."
            prop_row name:"min / max" values:"number" about:"reported range (default 0..1)."
            prop_row name:"step" values:"number" about:"quantize to multiples (0 = continuous)."
            prop_row name:"label" values:"text" about:"optional caption stacked above the track."
