[view]
col gap:24
    col gap:10
        text "▲ rsx — native UI for Rust" size:34 color:ink
        text "A tour of every primitive the framework ships, from a single box to spring-driven motion. The whole interface is written in .rsx markup; only non-visual logic lives in Rust." size:15 color:muted max_width:780
    grid cols:"fit 150" gap:16
        stat value:"25+" label:"modular crates"
        stat value:"2" label:"render backends"
        stat value:"60 fps" label:"software + wgpu"
        stat value:"0" label:"GC pauses"
    grid cols:"fit 200" gap:16
        feature_card icon:"🧩" title:"Declarative" body:"Rows, columns, boxes and text compose by indentation — no builder soup."
        feature_card icon:"🎨" title:"Themeable" body:"Semantic color tokens resolve reactively. Switch the theme in the sidebar."
        feature_card icon:"⚡" title:"Reactive" body:"Fine-grained signals update only the widgets that depend on them."
        feature_card icon:"📐" title:"Flex + grid" body:"A full flexbox and CSS-grid engine with wrapping and responsive tracks."
