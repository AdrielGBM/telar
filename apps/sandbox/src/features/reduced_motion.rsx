[logic]
use crate::shared::components::card::{card, CardProps};
use crate::shared::components::code_line::{code_line, CodeLineProps};
use crate::shared::components::doc_header::{doc_header, DocHeaderProps};
use crate::shared::components::example::{example, ExampleProps};
use crate::shared::components::prop_row::{prop_row, PropRowProps};

fn reading(value: Option<bool>) -> String {
    match value {
        Some(true) => "less motion".to_string(),
        Some(false) => "full motion".to_string(),
        None => "unknown".to_string(),
    }
}

let system = memo(|| reading(telar::use_system_reduced_motion()));
let chosen = memo(|| match telar::use_reduced_motion_override() {
    None => "follow the system".to_string(),
    forced => reading(forced),
});
let effective = memo(|| reading(telar::use_reduced_motion()));
let pulse = signal(0.0f32);

[view]
col gap:20
    doc_header kicker:"FOUNDATIONS" title:"Reduced motion" desc:"Less motion is the system's to say until the person says otherwise in the app. An override wins over the system while it is set, holds through every change the system reports, and is kept between runs; clearing it hands the choice back."
    example title:"What the app runs with"
        card gap:8
            text "system · {$system}" font_size:15 color:$theme.ink
            text "the app's choice · {$chosen}" font_size:15 color:$theme.ink
            text "in effect · {$effective}" font_size:15 color:$theme.primary
            row gap:8 wrap
                button label:"Follow the system" ghost on_press:(|| telar::set_reduced_motion_override(None))
                button label:"Less motion" ghost on_press:(|| telar::set_reduced_motion_override(Some(true)))
                button label:"Full motion" ghost on_press:(|| telar::set_reduced_motion_override(Some(false)))
            text "The choice is kept in the target's preference store: a file in the app's config directory, or localStorage on the web." font_size:12 color:$theme.muted
        code_line code:"set_reduced_motion_override(Some(true))   ·   use_reduced_motion()   ·   use_system_reduced_motion()"
    example title:"Every animation follows the value in effect"
        card gap:12
            row gap:12 align:center
                button label:"Slide" fill:$theme.primary on_press:(|| $pulse.set(if $pulse.get() < 0.5 { 1.0 } else { 0.0 }))
                spinner size:20
            box fill:$theme.surface_alt radius:6 height:12 width:260
                box fill:$theme.primary radius:6 height:12 width:40 translate_x:($pulse * 220.0) transition(translate_x 1200ms ease-in-out)
            text "With less motion in effect the bar jumps straight to its end and the spinner rests; scroll momentum keeps moving." font_size:12 color:$theme.muted
        code_line code:"motion::follow_reduced_motion(false)   // only for an app that tones its motion down itself"
    example title:"API"
        col gap:6
            prop_row name:"set_reduced_motion_override" values:"Option<bool>" about:"Some(true) less motion, Some(false) full motion, None follows the system."
            prop_row name:"use_reduced_motion" values:"Option<bool>" about:"the value in effect, read reactively; None only while both are unknown."
            prop_row name:"use_system_reduced_motion" values:"Option<bool>" about:"what the system reported, ignoring the override."
            prop_row name:"use_reduced_motion_override" values:"Option<bool>" about:"the app's choice itself."
