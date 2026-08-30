[logic]
use crate::shared::components::card::{card, CardProps};
use crate::shared::components::code_line::{code_line, CodeLineProps};
// A documented example block: a small title above its demo and snippet. Replaces the repeated
// `col gap:8 > text "…" font_size:13 color:theme.ink > …children…` scaffold used across every feature section,
// so a block is one `example title:"…"` line plus its content instead of two lines of boilerplate.
#[derive(Default)]
pub struct Props {
    pub title: &'static str,
}

[view]
col gap:8
    text "{props.title}" font_size:13 color:theme.ink
    children

[preview "Example"]
example title:"justify — distribute along the main axis"
    card gap:10
        text "The demo and its code snippet stack here." font_size:13 color:theme.muted
    code_line code:"row justify:between"
