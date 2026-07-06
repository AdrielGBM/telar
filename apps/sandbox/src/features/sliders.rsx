[logic]
let volume = signal(0.4f32);
let pct = memo(move || (volume.get() * 100.0) as i32);

[view]
col gap:20
    doc_header kicker:"17 · INTERACTION" title:"Slider" desc:"A drag-driven 0..1 control built on the on_drag primitive — the track, the filled portion, and the thumb are wired up for you; drop in a signal and read it back."
    col gap:8
        text "value:signal drives the fill and the thumb position" size:13 color:ink
        card gap:8
            slider value:$volume width:260
            text "Volume · {$pct}%" size:14 color:muted
        code_line code:"slider value:$volume width:260   (built on box on_drag — see Reactivity)"
