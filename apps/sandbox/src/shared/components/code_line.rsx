[logic]
// A one-line `.rsx` code snippet in a dark pill. Snippets use single quotes for string
// literals (real `.rsx` uses double quotes) because a `"` would close the markup string early.
pub struct Props {
    pub code: &'static str,
}

[view]
box fill:code_bg radius:8 pad_x:12 pad_y:8
    text "{props.code}" size:12 color:code_fg

[preview "Code line"]
code_line code:"box fill:primary radius:8 width:120 height:80"
