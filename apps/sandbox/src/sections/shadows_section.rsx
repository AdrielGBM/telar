[view]
col gap:8
    text "Shadows" size:12 color:muted
    col gap:16
        text "Rect shadows — offset / blur" size:11 color:muted
        row gap:16
            col gap:8 align:center
                box fill:white radius:8 width:152 height:80 shadow-y:4 shadow-blur:12 shadow-color:#00000040
                text "soft (0, 4, 12)" size:11 color:muted
            col gap:8 align:center
                box fill:white radius:8 width:152 height:80 shadow-x:4 shadow-y:8 shadow-blur:4 shadow-color:#00000066
                text "offset (4, 8, 4)" size:11 color:muted
            col gap:8 align:center
                box fill:white radius:8 width:152 height:80 shadow-y:6 shadow-blur:16 shadow-color:primary
                text "colored primary" size:11 color:muted
        text "Colored card glows" size:11 color:muted
        row gap:16
            col gap:8 align:center
                box fill:primary radius:10 width:168 height:80 shadow-y:8 shadow-blur:20 shadow-color:primary align:center justify:center
                    text "primary" size:12 color:on_color
            col gap:8 align:center
                box fill:success radius:10 width:168 height:80 shadow-y:8 shadow-blur:20 shadow-color:success align:center justify:center
                    text "success" size:12 color:on_color
            col gap:8 align:center
                box fill:danger radius:10 width:168 height:80 shadow-y:8 shadow-blur:20 shadow-color:danger align:center justify:center
                    text "danger" size:12 color:on_color
            col gap:8 align:center
                box fill:purple radius:10 width:168 height:80 shadow-y:8 shadow-blur:20 shadow-color:purple align:center justify:center
                    text "purple" size:12 color:on_color
