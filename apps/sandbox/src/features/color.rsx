[view]
col gap:20
    doc_header kicker:"04 · FOUNDATIONS" title:"Color & theme" desc:"Colors are semantic tokens, not fixed values. Every token resolves through the active theme — switch it in the sidebar and the whole app recolors reactively."
    col gap:8
        text "Accent tokens" size:13 color:ink
        card
            grid cols:"fit 120" gap:12
                col gap:6 align:center
                    box fill:primary radius:10 width:100% height:52
                    text "primary" size:12 color:ink
                col gap:6 align:center
                    box fill:success radius:10 width:100% height:52
                    text "success" size:12 color:ink
                col gap:6 align:center
                    box fill:danger radius:10 width:100% height:52
                    text "danger" size:12 color:ink
                col gap:6 align:center
                    box fill:warning radius:10 width:100% height:52
                    text "warning" size:12 color:ink
                col gap:6 align:center
                    box fill:purple radius:10 width:100% height:52
                    text "purple" size:12 color:ink
                col gap:6 align:center
                    box fill:cyan radius:10 width:100% height:52
                    text "cyan" size:12 color:ink
        code_line code:"box fill:primary   ·   fill:success   ·   fill:danger …"
    col gap:8
        text "Neutrals & surfaces (outlined so light tones stay visible)" size:13 color:ink
        card
            grid cols:"fit 120" gap:12
                col gap:6 align:center
                    box fill:ink radius:10 width:100% height:52 stroke:border
                    text "ink" size:12 color:ink
                col gap:6 align:center
                    box fill:muted radius:10 width:100% height:52 stroke:border
                    text "muted" size:12 color:ink
                col gap:6 align:center
                    box fill:surface radius:10 width:100% height:52 stroke:border
                    text "surface" size:12 color:ink
                col gap:6 align:center
                    box fill:surface_alt radius:10 width:100% height:52 stroke:border
                    text "surface_alt" size:12 color:ink
                col gap:6 align:center
                    box fill:border radius:10 width:100% height:52 stroke:border
                    text "border" size:12 color:ink
                col gap:6 align:center
                    box fill:background radius:10 width:100% height:52 stroke:border
                    text "background" size:12 color:ink
        code_line code:"box fill:surface stroke:border   (card recipe)"
    col gap:8
        text "One-off colors — inline hex when a token does not fit" size:13 color:ink
        card
            row gap:12
                box fill:#ff6b6b radius:8 width:64 height:40
                box fill:#4ecdc4 radius:8 width:64 height:40
                box fill:#ffe66d radius:8 width:64 height:40
        code_line code:"box fill:#4ecdc4      (also #rgb and #rrggbbaa)"
    col gap:8
        text "Reactive theming" size:13 color:ink
        card gap:8
            text "Because color:primary compiles to a theme lookup, swapping the theme struct at runtime updates every widget that reads it — no manual repaint." size:13 color:muted
            text "Try the Modern / Pastel / Midnight buttons in the sidebar." size:13 color:primary
        code_line code:"on_press:|| set_theme_with_widgets(SandboxTheme::midnight())"
