[view]
col gap:8
    text "Layers" size:12 color:muted
    col gap:16
        text "Opacity — same red rect at 1.0 / 0.6 / 0.3 / 0.1" size:11 color:muted
        row gap:16
            col gap:4 align:center
                box fill:danger radius:8 width:168 height:80 opacity:1.0 align:center justify:center
                    text "1.0" size:18 color:on_color
            col gap:4 align:center
                box fill:danger radius:8 width:168 height:80 opacity:0.6 align:center justify:center
                    text "0.6" size:18 color:on_color
            col gap:4 align:center
                box fill:danger radius:8 width:168 height:80 opacity:0.3 align:center justify:center
                    text "0.3" size:18 color:on_color
            col gap:4 align:center
                box fill:danger radius:8 width:168 height:80 opacity:0.1 align:center justify:center
                    text "0.1" size:18 color:on_color
        row gap:16
            col gap:4 grow:1
                text "Gradient + layer opacity 0.8" size:11 color:muted
                box gradient:horizontal from:primary mid:purple to:danger radius:12 height:80 opacity:0.8 align:center justify:center
                    text "gradient + opacity 0.8" size:18 color:on_color
            col gap:4 grow:1
                text "Nested: outer 0.6, inner 0.5 — combined ~0.3" size:11 color:muted
                box fill:primary radius:8 height:120 opacity:0.6 pad:16 gap:8
                    text "outer 0.6" size:11 color:on_color
                    box fill:danger radius:6 grow:1 opacity:0.5 align:center justify:center
                        text "inner 0.5" size:14 color:on_color
