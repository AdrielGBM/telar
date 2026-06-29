[view]
section "Shadows"
    col gap:16
        text "Rect shadows — offset / blur" size:11 color:muted
        row gap:16
            col gap:8 align:center
                box fill:white radius:8 width:152 height:80 shadow_y:4 shadow_blur:12 shadow_color:#00000040
                text "soft (0, 4, 12)" size:11 color:muted
            col gap:8 align:center
                box fill:white radius:8 width:152 height:80 shadow_x:4 shadow_y:8 shadow_blur:4 shadow_color:#00000066
                text "offset (4, 8, 4)" size:11 color:muted
            col gap:8 align:center
                box fill:white radius:8 width:152 height:80 shadow_y:6 shadow_blur:16 shadow_color:primary
                text "colored primary" size:11 color:muted
        text "Colored card glows" size:11 color:muted
        row gap:16
            col gap:8 align:center
                box fill:primary radius:10 width:168 height:80 shadow_y:8 shadow_blur:20 shadow_color:primary align:center justify:center
                    text "primary" size:12 color:on_color
            col gap:8 align:center
                box fill:success radius:10 width:168 height:80 shadow_y:8 shadow_blur:20 shadow_color:success align:center justify:center
                    text "success" size:12 color:on_color
            col gap:8 align:center
                box fill:danger radius:10 width:168 height:80 shadow_y:8 shadow_blur:20 shadow_color:danger align:center justify:center
                    text "danger" size:12 color:on_color
            col gap:8 align:center
                box fill:purple radius:10 width:168 height:80 shadow_y:8 shadow_blur:20 shadow_color:purple align:center justify:center
                    text "purple" size:12 color:on_color
