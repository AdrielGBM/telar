[logic]
// A panel drawn in a theme of its own. What a call site nests inside it is built where `children` stands, so it reads the panel's theme and its `Context` exactly as a child written inline would.
#[derive(Clone, Copy)]
pub struct Context {
    pub mode: &'static str,
}

pub struct Props {
    pub mode: &'static str = "midnight",
}

let mode = props.mode;
let palette = follow_theme(move || crate::core::theme::SandboxTheme::by_mode(mode));
let ctx = Context { mode };

[view]
col theme:(palette) fill:$theme.surface pad:14 radius:10
    col gap:6
        children in:ctx

[preview "Themed panel"]
themed_panel mode:"pastel"
    text "Nested at the call site, drawn in {$theme.name}" font_size:14 color:$theme.ink
