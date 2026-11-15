[logic]
use crate::shared::components::card::{CardProps, card};
use crate::shared::components::code_line::{CodeLineProps, code_line};
#[derive(Default)]
pub struct Props {
    pub title: &'static str,
    #[props(default)]
    pub children: Children,
}

[view]
col gap:8
    text "{props.title}" font_size:13 color:$theme.ink
    children $props.children

[previews "Sandbox/Example" layout:padded]
A titled block that stacks a demo above its code snippet.

[preview "Example"]
example title:"justify — distribute along the main axis"
    card gap:10
        text "The demo and its code snippet stack here." font_size:13 color:$theme.muted
    code_line code:"row justify:between"
