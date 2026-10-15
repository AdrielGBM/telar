[logic]
use crate::shared::components::feature_card::{FeatureCardProps, feature_card};
use crate::shared::components::stat::{StatProps, stat};

[view]
col gap:24
    col gap:10
        row gap:12 align:center
            icon name:"lucide:triangle" size:30 color:$theme.ink
            text "rsx — native UI for Rust" font_size:34 color:$theme.ink
        text "A tour of every primitive the framework ships, from a single box to spring-driven motion. The whole interface is written in .rsx markup; only non-visual logic lives in Rust." font_size:15 color:$theme.muted max_width:780
    grid cols:"fit 150" gap:16
        stat value:"25+" label:"modular crates"
        stat value:"2" label:"render backends"
        stat value:"60 fps" label:"software + wgpu"
        stat value:"0" label:"GC pauses"
    grid cols:"fit 200" gap:16
        feature_card title:"Declarative" body:"Rows, columns, boxes and text compose by indentation — no builder soup."
            icon name:"lucide:puzzle" size:28 color:$theme.primary slot:"icon"
        feature_card title:"Themeable" body:"Semantic color tokens resolve reactively. Switch the theme in the sidebar."
            icon name:"lucide:palette" size:28 color:$theme.primary slot:"icon"
        feature_card title:"Reactive" body:"Fine-grained signals update only the widgets that depend on them."
            icon name:"lucide:zap" size:28 color:$theme.primary slot:"icon"
        feature_card title:"Flex + grid" body:"A full flexbox and CSS-grid engine with wrapping and responsive tracks."
            icon name:"lucide:layout-grid" size:28 color:$theme.primary slot:"icon"
