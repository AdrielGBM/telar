[view]
col gap:20
    doc_header kicker:"07 · SURFACES" title:"Shadows" desc:"Drop shadows take an offset, a blur radius, and a color. Use a soft neutral shadow for elevation or a matching color for a glow."

    col gap:8
        text "Elevation — offset and blur lift a card off the page" size:13 color:ink
        box fill:surface_alt stroke:border radius:12 pad:24
            row gap:20 wrap align:center
                col gap:8 align:center
                    box fill:surface radius:10 width:150 height:76 shadow_y:2 shadow_blur:6 shadow_color:#0000002e
                    text "low" size:12 color:muted
                col gap:8 align:center
                    box fill:surface radius:10 width:150 height:76 shadow_y:6 shadow_blur:16 shadow_color:#00000033
                    text "medium" size:12 color:muted
                col gap:8 align:center
                    box fill:surface radius:10 width:150 height:76 shadow_y:12 shadow_blur:28 shadow_color:#0000003d
                    text "high" size:12 color:muted
        code_line code:"box shadow_y:6 shadow_blur:16 shadow_color:#00000033"

    col gap:8
        text "Colored glows — a shadow the same hue as the fill" size:13 color:ink
        box fill:surface stroke:border radius:12 pad:24
            row gap:20 wrap
                box fill:primary radius:12 width:150 height:72 shadow_y:8 shadow_blur:22 shadow_color:primary align:center justify:center
                    text "primary" size:12 color:on_primary
                box fill:danger radius:12 width:150 height:72 shadow_y:8 shadow_blur:22 shadow_color:danger align:center justify:center
                    text "danger" size:12 color:on_primary
                box fill:purple radius:12 width:150 height:72 shadow_y:8 shadow_blur:22 shadow_color:purple align:center justify:center
                    text "purple" size:12 color:on_primary
        code_line code:"box fill:primary shadow_y:8 shadow_blur:22 shadow_color:primary"

    col gap:8
        text "Offset — push the shadow on the x and y axes" size:13 color:ink
        box fill:surface stroke:border radius:12 pad:24
            box fill:surface_alt radius:10 width:170 height:80 shadow_x:8 shadow_y:8 shadow_blur:4 shadow_color:warning
        code_line code:"box shadow_x:8 shadow_y:8 shadow_blur:4 shadow_color:warning"

    col gap:8
        text "Attributes" size:13 color:ink
        col gap:6
            prop_row name:"shadow_x / shadow_y" values:"number" about:"Shadow offset (default 0 / 4)."
            prop_row name:"shadow_blur" values:"number" about:"Blur radius (default 8)."
            prop_row name:"shadow_color" values:"token · #rrggbbaa" about:"Shadow color; use alpha for softness."
