[view]
col gap:20
    doc_header kicker:"08 · SURFACES" title:"Opacity & layers" desc:"opacity fades a box and everything inside it as a single layer, so nested transparencies multiply the way you would expect."

    col gap:8
        text "A single fill at descending opacity" size:13 color:ink
        box fill:surface stroke:border radius:12 pad:16
            row gap:14 wrap
                box fill:danger radius:10 width:120 height:72 opacity:1.0 align:center justify:center
                    text "1.0" size:16 color:on_primary
                box fill:danger radius:10 width:120 height:72 opacity:0.6 align:center justify:center
                    text "0.6" size:16 color:on_primary
                box fill:danger radius:10 width:120 height:72 opacity:0.3 align:center justify:center
                    text "0.3" size:16 color:on_primary
                box fill:danger radius:10 width:120 height:72 opacity:0.1 align:center justify:center
                    text "0.1" size:16 color:on_primary
        code_line code:"box fill:danger opacity:0.3"

    col gap:8
        text "Layer opacity applies to a gradient and its label together" size:13 color:ink
        box fill:surface stroke:border radius:12 pad:16
            box gradient:horizontal from:primary mid:purple to:danger radius:12 height:80 opacity:0.75 align:center justify:center
                text "gradient at 0.75" size:16 color:on_primary
        code_line code:"box gradient:horizontal from:primary to:danger opacity:0.75"

    col gap:8
        text "Nested — outer 0.6 × inner 0.6 combine to about 0.36" size:13 color:ink
        box fill:surface stroke:border radius:12 pad:16
            box fill:primary radius:10 height:120 opacity:0.6 pad:14 gap:8
                text "outer · 0.6" size:12 color:on_primary
                box fill:danger radius:8 grow:1 opacity:0.6 align:center justify:center
                    text "inner · 0.6" size:14 color:on_primary
        code_line code:"box opacity:0.6   >   box opacity:0.6   (layers multiply)"
