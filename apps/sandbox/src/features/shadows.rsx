[style]
@center
    align: center
    justify: center

[view]
col gap:20
    doc_header kicker:"SURFACES" title:"Shadows" desc:"Drop shadows take an offset, a blur radius, and a color. Use a soft neutral shadow for elevation or a matching color for a glow."
    example title:"Elevation — offset and blur lift a card off the page"
        box fill:surface_alt stroke:border radius:12 pad:24
            row gap:20 wrap align:center
                col gap:8 align:center
                    box fill:surface radius:10 width:150 height:76 shadow_y:2 shadow_blur:6 shadow_color:#0000002e
                    text "low" font_size:12 color:muted
                col gap:8 align:center
                    box fill:surface radius:10 width:150 height:76 shadow_y:6 shadow_blur:16 shadow_color:#00000033
                    text "medium" font_size:12 color:muted
                col gap:8 align:center
                    box fill:surface radius:10 width:150 height:76 shadow_y:12 shadow_blur:28 shadow_color:#0000003d
                    text "high" font_size:12 color:muted
        code_line code:"box shadow_y:6 shadow_blur:16 shadow_color:#00000033"
    example title:"Colored glows — a shadow the same hue as the fill"
        card pad:24
            row gap:20 wrap
                box @center fill:primary radius:12 width:150 height:72 shadow_y:8 shadow_blur:22 shadow_color:primary
                    text "primary" font_size:12 color:on_primary
                box @center fill:danger radius:12 width:150 height:72 shadow_y:8 shadow_blur:22 shadow_color:danger
                    text "danger" font_size:12 color:on_primary
                box @center fill:purple radius:12 width:150 height:72 shadow_y:8 shadow_blur:22 shadow_color:purple
                    text "purple" font_size:12 color:on_primary
        code_line code:"box fill:primary shadow_y:8 shadow_blur:22 shadow_color:primary"
    example title:"Offset — push the shadow on the x and y axes"
        card pad:24
            box fill:surface_alt radius:10 width:170 height:80 shadow_x:8 shadow_y:8 shadow_blur:4 shadow_color:warning
        code_line code:"box shadow_x:8 shadow_y:8 shadow_blur:4 shadow_color:warning"
    example title:"Attributes"
        col gap:6
            prop_row name:"shadow_x / shadow_y" values:"number" about:"Shadow offset (default 0 / 4)."
            prop_row name:"shadow_blur" values:"number" about:"Blur radius (default 8)."
            prop_row name:"shadow_color" values:"token · #rrggbbaa" about:"Shadow color; use alpha for softness."
