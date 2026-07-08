[style]
@center
    align: center
    justify: center

[view]
col gap:20
    doc_header kicker:"SURFACES" title:"Boxes & borders" desc:"A box is a styled container: give it a fill, a stroke, and a corner radius. Those same paint attributes work on any row or col too."
    example title:"Fill, stroke, and both"
        card
            row gap:14 wrap
                box @center fill:primary radius:10 width:150 height:70
                    text "fill" size:13 color:on_primary
                box @center stroke:primary stroke_width:2 radius:10 width:150 height:70
                    text "stroke" size:13 color:primary
                box @center fill:surface_alt stroke:primary stroke_width:2 radius:10 width:150 height:70
                    text "fill + stroke" size:13 color:ink
        code_line code:"box fill:primary radius:10      box stroke:primary stroke_width:2"
    example title:"Corner radius — from sharp to a full pill"
        card
            row gap:14 wrap align:center
                box @center fill:purple radius:0 width:110 height:64
                    text "0" size:13 color:on_primary
                box @center fill:purple radius:8 width:110 height:64
                    text "8" size:13 color:on_primary
                box @center fill:purple radius:20 width:110 height:64
                    text "20" size:13 color:on_primary
                box @center fill:purple radius:40 width:130 height:56
                    text "pill" size:13 color:on_primary
        code_line code:"box radius:0   ·   radius:8   ·   radius:20   ·   radius:40"
    example title:"Stroke width — a plain box makes a hairline or a heavy border"
        card
            row gap:14 wrap align:center
                box stroke:success stroke_width:1 radius:8 width:120 height:56
                box stroke:success stroke_width:2 radius:8 width:120 height:56
                box stroke:success stroke_width:4 radius:8 width:120 height:56
        code_line code:"box stroke:success stroke_width:4"
    example title:"Content alignment inside a box"
        card
            box @center fill:surface_alt radius:10 width:100% height:96
                text "align:center justify:center" size:13 color:ink
        code_line code:"box align:center justify:center   (a box is a flex column)"
    example title:"Attributes"
        col gap:6
            prop_row name:"fill" values:"token · #hex · $signal" about:"Solid background color."
            prop_row name:"stroke" values:"token · #hex" about:"Border color (pair with stroke_width)."
            prop_row name:"stroke_width" values:"number" about:"Border thickness (default 1)."
            prop_row name:"radius" values:"number" about:"Corner radius on all four corners."
            prop_row name:"opacity" values:"0–1 · $signal" about:"Fades the box and its children as a layer."
