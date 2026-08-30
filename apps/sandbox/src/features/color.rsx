[logic]
use crate::shared::components::card::{card, CardProps};
use crate::shared::components::code_line::{code_line, CodeLineProps};
use crate::shared::components::doc_header::{doc_header, DocHeaderProps};
use crate::shared::components::example::{example, ExampleProps};

[style]
@swatch
    gap: 6
    align: center

@chip
    radius: 10
    width: 100%
    height: 52

[view]
col gap:20
    doc_header kicker:"FOUNDATIONS" title:"Color & theme" desc:"Colors are semantic tokens, not fixed values. Every token resolves through the active theme — switch it in the sidebar and the whole app recolors reactively."
    example title:"Accent tokens"
        card
            grid cols:"fit 120" gap:12 font_size:12 color:theme.ink
                col @swatch
                    box @chip fill:theme.primary
                    text "primary"
                col @swatch
                    box @chip fill:theme.success
                    text "success"
                col @swatch
                    box @chip fill:theme.danger
                    text "danger"
                col @swatch
                    box @chip fill:theme.warning
                    text "warning"
                col @swatch
                    box @chip fill:theme.purple
                    text "purple"
                col @swatch
                    box @chip fill:theme.cyan
                    text "cyan"
        code_line code:"box fill:theme.primary   ·   fill:theme.success   ·   fill:theme.danger …"
    example title:"Neutrals & surfaces (outlined so light tones stay visible)"
        card
            grid cols:"fit 120" gap:12 font_size:12 color:theme.ink
                col @swatch
                    box @chip fill:theme.ink stroke:theme.border
                    text "ink"
                col @swatch
                    box @chip fill:theme.muted stroke:theme.border
                    text "muted"
                col @swatch
                    box @chip fill:theme.surface stroke:theme.border
                    text "surface"
                col @swatch
                    box @chip fill:theme.surface_alt stroke:theme.border
                    text "surface_alt"
                col @swatch
                    box @chip fill:theme.border stroke:theme.border
                    text "border"
                col @swatch
                    box @chip fill:theme.background stroke:theme.border
                    text "background"
        code_line code:"box fill:theme.surface stroke:theme.border   (card recipe)"
    example title:"One-off colors — inline hex when a token does not fit"
        card
            row gap:12
                box fill:#ff6b6b radius:8 width:64 height:40
                box fill:#4ecdc4 radius:8 width:64 height:40
                box fill:#ffe66d radius:8 width:64 height:40
        code_line code:"box fill:#4ecdc4      (also #rgb and #rrggbbaa)"
    example title:"Reactive theming"
        card gap:8
            text "Because color:theme.primary compiles to a theme lookup, swapping the theme struct at runtime updates every widget that reads it — no manual repaint." font_size:13 color:theme.muted
            text "Try the Modern / Pastel / Midnight buttons in the sidebar." font_size:13 color:theme.primary
        code_line code:"on_press:|| set_mode(\"midnight\")"
