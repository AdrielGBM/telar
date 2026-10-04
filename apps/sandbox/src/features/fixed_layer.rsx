[logic]
use crate::shared::components::card::{card, CardProps};
use crate::shared::components::code_line::{code_line, CodeLineProps};
use crate::shared::components::doc_header::{doc_header, DocHeaderProps};
use crate::shared::components::example::{example, ExampleProps};

let shown = signal(false);
let taps = signal(0u32);
let primary = memo(|| match use_primary_scroll() {
    Some(page) => format!("{}% of the page", (page.progress(Axis::Vertical) * 100.0).round()),
    None => "none: each section here scrolls itself".to_string(),
});

[view]
col gap:20
    doc_header kicker:"FOUNDATIONS" title:"Fixed layer" desc:"A layer stands against the surface over the page, out of its flow and its scroll: the page scrolls under it and its sticky boxes never cover it. It is not a modal. It takes the pointer only over its own boxes, so the rest of the surface still reaches the page, and its controls keep their place in the Tab order, where the layer is declared."
    example title:"layer — a bar fixed over the surface"
        card gap:12
            toggle checked:$shown label:"show the bar"
            text "Taps on the bar: {$taps}" font_size:13 color:$theme.muted
            text "Scroll this section, or click beside the bar: the page under it still answers. Tab from the toggle reaches the bar's button next." font_size:12 color:$theme.muted
            if $shown
                layer justify:end align:center pad:24
                    row gap:12 pad_x:16 pad_y:10 radius:12 fill:$theme.surface stroke:$theme.border align:center label:"Fixed bar"
                        text "Fixed over the surface" font_size:13 color:$theme.ink
                        button label:"Tap" fill:$theme.primary on_press:(|| $taps.update(|n| *n += 1))
        code_line code:"layer justify:end align:center pad:24  >  row …     (laid out against the whole surface)"
    example title:"use_primary_scroll() — the page's scroll, read from outside its content"
        card gap:12
            text "Primary scroll: {$primary}" font_size:13 color:$theme.ink
        code_line code:"memo(|| use_primary_scroll().map_or(0.0, |page| page.progress(Axis::Vertical)))"
    example title:"arrival_margin — a place arrived at lands below the bar, not under it"
        card gap:12
            scroll height:260 width:100% arrival_margin_top:40
                col width:100%
                    row sticky inset_top:0 height:40 width:100% gap:8 pad_x:12 align:center fill:$theme.surface_alt label:"Bar over the sections"
                        box to:anchor("arrival-one") fill:$theme.surface radius:6 pad_x:10 pad_y:4 hover_style(fill:$theme.border)
                            text "One" font_size:12 color:$theme.ink
                        box to:anchor("arrival-two") fill:$theme.surface radius:6 pad_x:10 pad_y:4 hover_style(fill:$theme.border)
                            text "Two" font_size:12 color:$theme.ink
                        box to:anchor("arrival-three") fill:$theme.surface radius:6 pad_x:10 pad_y:4 hover_style(fill:$theme.border)
                            text "Three" font_size:12 color:$theme.ink
                    col anchor:"arrival-one" width:100% gap:6 pad:12
                        text "Section one" font_size:13 font_weight:600 color:$theme.ink
                        for i in 0..6
                            text "Row {i} of section one" font_size:12 color:$theme.muted
                    col anchor:"arrival-two" width:100% gap:6 pad:12
                        text "Section two" font_size:13 font_weight:600 color:$theme.ink
                        for i in 0..6
                            text "Row {i} of section two" font_size:12 color:$theme.muted
                    col anchor:"arrival-three" width:100% gap:6 pad:12
                        text "Section three" font_size:13 font_weight:600 color:$theme.ink
                        for i in 0..6
                            text "Row {i} of section three" font_size:12 color:$theme.muted
            text "This scroll declares arrival_margin_top:40, the height of the bar stuck over it: a link in the bar brings its section to just below the bar instead of under it, and so does a control Tab brings into view. A page needs no declaration: its margin is the reach of the bars of the layers fixed over it, the boxes of a layer that stand against the top or the bottom of the surface." font_size:12 color:$theme.muted
        code_line code:"scroll arrival_margin_top:40   ·   ScrollPage::arrival_margin(|| Insets::new(56.0, 0.0, 0.0, 0.0))"
