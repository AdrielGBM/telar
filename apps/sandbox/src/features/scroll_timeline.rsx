[logic]
use crate::shared::components::card::{card, CardProps};
use crate::shared::components::code_line::{code_line, CodeLineProps};
use crate::shared::components::doc_header::{doc_header, DocHeaderProps};
use crate::shared::components::example::{example, ExampleProps};

let scene = signal(0.0f32);
let arrival = signal(0.0f32);

let colors = motion::Timeline::<Color>::builder(theme.get().primary)
    .then(
        theme.get().purple,
        std::time::Duration::from_millis(500),
        motion::Easing::Linear,
    )
    .then(
        theme.get().success,
        std::time::Duration::from_millis(500),
        motion::Easing::Linear,
    )
    .build();
let stage_color = memo(move || colors.sample(scene.get()));
let stage_scale = memo(move || 0.6 + 0.4 * scene.get());

[view]
col gap:20
    doc_header kicker:"INTERACTION" title:"Scroll timelines" desc:"How far a box has travelled through the view of the scroll it sits in, as a progress from 0 to 1 that a Timeline samples. Computed from Telar's own layout and the scroll's offset, so every target plays it the same: the document's scroll on the web, a scroll area anywhere else."
    example title:"A scene — the stage plays while its track fills the view"
        card gap:12
            scroll height:240 width:100%
                col width:100% gap:12
                    text "Scroll: the stage holds still and plays as its track passes under it." font_size:12 color:$theme.muted
                    col height:800 width:100% view_progress:$scene view_range:contain
                        box fill:$stage_color radius:10 height:160 width:100% sticky inset_top:40 align:center justify:center
                            box scale:$stage_scale
                                text "p = {($scene * 100.0).round() / 100.0}" font_size:16 color:$theme.on_primary
                    text "The track ended; so did the scene." font_size:12 color:$theme.muted
                    box height:160 width:100%
        code_line code:"col height:800 view_progress:$p view_range:contain  >  box sticky inset_top:40 fill:$tl.sample(p)"
    example title:"Entry — a box fading in as it arrives"
        card gap:12
            scroll height:200 width:100%
                col width:100% gap:12
                    box height:260 width:100%
                        text "Scroll down." font_size:12 color:$theme.muted
                    box fill:$theme.primary radius:10 height:80 width:100% opacity:$arrival view_progress:$arrival view_range:entry align:center justify:center
                        text "arrived" font_size:14 color:$theme.on_primary
                    box height:120 width:100%
        code_line code:"box view_progress:$p view_range:entry opacity:$p     ·   cover · contain · entry · exit"
