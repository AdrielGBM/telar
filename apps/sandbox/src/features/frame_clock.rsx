[logic]
use crate::shared::components::card::{card, CardProps};
use crate::shared::components::code_line::{code_line, CodeLineProps};
use crate::shared::components::doc_header::{doc_header, DocHeaderProps};
use crate::shared::components::example::{example, ExampleProps};
use crate::shared::components::prop_row::{prop_row, PropRowProps};

let running = signal(false);
let time = motion::use_frame_time_while(move || running.get());
let swing = memo(move || (time.get().as_secs_f32() * std::f32::consts::TAU / 1.6).sin() * 90.0);
let seconds = memo(move || format!("{:.2} s", time.get().as_secs_f32()));
let state = memo(move || if running.get() { "reading the clock: frames are requested" } else { "not reading: the loop is asleep" });

[view]
col gap:20
    doc_header kicker:"INTERACTION" title:"Frame clock" desc:"A reactive elapsed time for motion that is a function of time rather than a transition toward a target. It advances only while a reactive reader is subscribed, so a screen that stops reading it asks for no frames, and it freezes under reduced motion."
    example title:"An idle sway that exists only while it is read"
        card gap:12
            row gap:12 align:center
                button label:"Start" fill:$theme.primary on_press:(|| $running.set(true))
                button label:"Stop" ghost on_press:(|| $running.set(false))
                text "{$seconds}" font_size:15 color:$theme.ink
            box height:24 width:260 align:center
                box fill:$theme.primary radius:12 width:24 height:24 translate_x:$swing
            text "{$state}" font_size:12 color:$theme.muted
            text "Stop holds the reading, and Start continues from it. With less motion in effect the sway rests wherever the clock stood, because the clock freezes." font_size:12 color:$theme.muted
        code_line code:"let time = motion::use_frame_time_while(move || running.get());"
    example title:"API"
        col gap:6
            prop_row name:"use_frame_time()" values:"ReadSignal<Duration>" about:"A clock tied to the calling scope, at zero when made. It runs while an effect, memo or view segment reads it."
            prop_row name:"use_frame_time_while(f)" values:"Memo<Duration>" about:"The same, read only while f() is true; otherwise it holds its last reading and asks for nothing."
            prop_row name:"FrameClock" values:"get · peek · millis · secs · reset" about:"The handle behind both. peek reads without subscribing."
            prop_row name:"MAX_FRAME_STEP" values:"100 ms" about:"The most one frame adds, so a hidden tab or a stall does not jump the clock."
