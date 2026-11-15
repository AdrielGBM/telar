[logic]
// Snippets quote strings with single quotes because a `"` would close the markup string early.
#[derive(Default)]
pub struct Props {
    pub code: &'static str,
}

[view]
box fill:$theme.code_bg radius:8 pad_x:12 pad_y:8
    text "{props.code}" font_size:12 color:$theme.code_fg

[previews "Sandbox/Code line" layout:padded]
A one-line code snippet in a dark pill, used under each documented example.

[preview "Code line"]
code_line code:"box fill:primary radius:8 width:120 height:80"
