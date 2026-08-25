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
            grid cols:"fit 120" gap:12
                col @swatch
                    box @chip fill:primary
                    text "primary" font_size:12 color:ink
                col @swatch
                    box @chip fill:success
                    text "success" font_size:12 color:ink
                col @swatch
                    box @chip fill:danger
                    text "danger" font_size:12 color:ink
                col @swatch
                    box @chip fill:warning
                    text "warning" font_size:12 color:ink
                col @swatch
                    box @chip fill:purple
                    text "purple" font_size:12 color:ink
                col @swatch
                    box @chip fill:cyan
                    text "cyan" font_size:12 color:ink
        code_line code:"box fill:primary   ·   fill:success   ·   fill:danger …"
    example title:"Neutrals & surfaces (outlined so light tones stay visible)"
        card
            grid cols:"fit 120" gap:12
                col @swatch
                    box @chip fill:ink stroke:border
                    text "ink" font_size:12 color:ink
                col @swatch
                    box @chip fill:muted stroke:border
                    text "muted" font_size:12 color:ink
                col @swatch
                    box @chip fill:surface stroke:border
                    text "surface" font_size:12 color:ink
                col @swatch
                    box @chip fill:surface_alt stroke:border
                    text "surface_alt" font_size:12 color:ink
                col @swatch
                    box @chip fill:border stroke:border
                    text "border" font_size:12 color:ink
                col @swatch
                    box @chip fill:background stroke:border
                    text "background" font_size:12 color:ink
        code_line code:"box fill:surface stroke:border   (card recipe)"
    example title:"One-off colors — inline hex when a token does not fit"
        card
            row gap:12
                box fill:#ff6b6b radius:8 width:64 height:40
                box fill:#4ecdc4 radius:8 width:64 height:40
                box fill:#ffe66d radius:8 width:64 height:40
        code_line code:"box fill:#4ecdc4      (also #rgb and #rrggbbaa)"
    example title:"Reactive theming"
        card gap:8
            text "Because color:primary compiles to a theme lookup, swapping the theme struct at runtime updates every widget that reads it — no manual repaint." font_size:13 color:muted
            text "Try the Modern / Pastel / Midnight buttons in the sidebar." font_size:13 color:primary
        code_line code:"on_press:|| set_mode(\"midnight\")"
