[logic]
use crate::shared::components::card::{card, CardProps};
use crate::shared::components::code_line::{code_line, CodeLineProps};
use crate::shared::components::doc_header::{doc_header, DocHeaderProps};
use crate::shared::components::example::{example, ExampleProps};
use crate::shared::components::prop_row::{prop_row, PropRowProps};
let level = signal(5i32);
let focused = signal(false);
let pressed = signal(0i32);

[view]
col gap:20
    doc_header kicker:"INTERACTION" title:"Keyboard" desc:"Each focusable control says which keys it keeps while it holds focus. On a web page the rest go back to the browser: Tab moves through the page's own focus order, and the arrows and Space scroll it. A custom control declares its keys with consumes_keys."
    example title:"consumes_keys — a custom control that keeps the arrows, beside plain buttons"
        card gap:12
            row gap:12 wrap align:center
                button label:"Before" fill:$theme.primary on_press:(|| $pressed += 1)
                box role:slider fill:$theme.surface_alt stroke:$theme.border radius:8 pad_x:16 pad_y:10 focus_style(stroke:$theme.primary) on_focus:(|now| $focused.set(now)) on_press:(|| ()) consumes_keys:arrows on_key:(|key: &Key| -> bool { if !$focused.get() { return false; } match key { Key::Named(NamedKey::ArrowRight | NamedKey::ArrowUp) => { $level.set(($level.get() + 1).min(10)); true } Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowDown) => { $level.set(($level.get() - 1).max(0)); true } _ => false } })
                    text "level {$level} / 10" font_size:13 color:$theme.ink
                button label:"After" outline:$theme.primary on_press:(|| $pressed += 1)
            text "Tab moves between all three. With the level focused, the arrows change it and the page does not scroll; on a button, the arrows scroll the page. Buttons pressed · {$pressed}" font_size:12 color:$theme.muted
        code_line code:"box role:slider focus_style(…) consumes_keys:arrows on_key:(|key| …)"
    example title:"Attributes"
        col gap:6
            prop_row name:"consumes_keys" values:"arrows · up · down · left · right · space · enter · tab · pageup · pagedown · home · end · activation · paging · edges · scrolling · none" about:"The keys this box keeps while focused, in place of what its role keeps. Names are comma- or space-separated; a $-reading expression that yields a ConsumedKeys is re-read every render."
