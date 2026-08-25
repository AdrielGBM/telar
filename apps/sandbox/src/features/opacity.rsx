[style]
@center
    align: center
    justify: center

[view]
col gap:20
    doc_header kicker:"SURFACES" title:"Opacity & layers" desc:"opacity fades a box and everything inside it as a single layer, so nested transparencies multiply the way you would expect."
    example title:"A single fill at descending opacity"
        card
            row gap:14 wrap font_size:16 color:theme.on_primary
                box @center fill:theme.danger radius:10 width:120 height:72 opacity:1.0
                    text "1.0"
                box @center fill:theme.danger radius:10 width:120 height:72 opacity:0.6
                    text "0.6"
                box @center fill:theme.danger radius:10 width:120 height:72 opacity:0.3
                    text "0.3"
                box @center fill:theme.danger radius:10 width:120 height:72 opacity:0.1
                    text "0.1"
        code_line code:"box fill:theme.danger opacity:0.3"
    example title:"Layer opacity applies to a gradient and its label together"
        card
            box @center fill:linear(horizontal, theme.primary, theme.purple, theme.danger) radius:12 height:80 opacity:0.75
                text "gradient at 0.75" font_size:16 color:theme.on_primary
        code_line code:"box fill:linear(horizontal, theme.primary, theme.danger) opacity:0.75"
    example title:"Nested — outer 0.6 × inner 0.6 combine to about 0.36"
        card
            box fill:theme.primary radius:10 height:120 opacity:0.6 pad:14 gap:8
                text "outer · 0.6" font_size:12 color:theme.on_primary
                box @center fill:theme.danger radius:8 grow:1 opacity:0.6
                    text "inner · 0.6" font_size:14 color:theme.on_primary
        code_line code:"box opacity:0.6   >   box opacity:0.6   (layers multiply)"
