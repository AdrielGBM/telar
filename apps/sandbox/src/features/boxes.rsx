[view]
col gap:20
    doc_header kicker:"05 · SURFACES" title:"Boxes & borders" desc:"A box is a styled container: give it a fill, a stroke, and a corner radius. Those same paint attributes work on any row or col too."

    col gap:8
        text "Fill, stroke, and both" size:13 color:ink
        card
            row gap:14 wrap
                box fill:primary radius:10 width:150 height:70 align:center justify:center
                    text "fill" size:13 color:on_primary
                box stroke:primary stroke_w:2 radius:10 width:150 height:70 align:center justify:center
                    text "stroke" size:13 color:primary
                box fill:surface_alt stroke:primary stroke_w:2 radius:10 width:150 height:70 align:center justify:center
                    text "fill + stroke" size:13 color:ink
        code_line code:"box fill:primary radius:10      box stroke:primary stroke_w:2"

    col gap:8
        text "Corner radius — from sharp to a full pill" size:13 color:ink
        card
            row gap:14 wrap align:center
                box fill:purple radius:0 width:110 height:64 align:center justify:center
                    text "0" size:13 color:on_primary
                box fill:purple radius:8 width:110 height:64 align:center justify:center
                    text "8" size:13 color:on_primary
                box fill:purple radius:20 width:110 height:64 align:center justify:center
                    text "20" size:13 color:on_primary
                box fill:purple radius:40 width:130 height:56 align:center justify:center
                    text "pill" size:13 color:on_primary
        code_line code:"box radius:0   ·   radius:8   ·   radius:20   ·   radius:40"

    col gap:8
        text "Stroke width — a plain box makes a hairline or a heavy border" size:13 color:ink
        card
            row gap:14 wrap align:center
                box stroke:success stroke_w:1 radius:8 width:120 height:56
                box stroke:success stroke_w:2 radius:8 width:120 height:56
                box stroke:success stroke_w:4 radius:8 width:120 height:56
        code_line code:"box stroke:success stroke_w:4"

    col gap:8
        text "Content alignment inside a box" size:13 color:ink
        card
            box fill:surface_alt radius:10 width:100% height:96 align:center justify:center
                text "align:center justify:center" size:13 color:ink
        code_line code:"box align:center justify:center   (a box is a flex column)"

    col gap:8
        text "Attributes" size:13 color:ink
        col gap:6
            prop_row name:"fill" values:"token · #hex · $signal" about:"Solid background color."
            prop_row name:"stroke" values:"token · #hex" about:"Border color (pair with stroke_w)."
            prop_row name:"stroke_w" values:"number" about:"Border thickness (default 1)."
            prop_row name:"radius" values:"number" about:"Corner radius on all four corners."
            prop_row name:"opacity" values:"0–1 · $signal" about:"Fades the box and its children as a layer."
