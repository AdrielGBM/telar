[view]
col gap:8
    text "Grid" size:12 color:muted
    col gap:16
        text "Auto-placed (repeat(3, 1fr))" size:12 color:muted
        grid cols:3 gap:12
            box fill:primary radius:6 height:72 align:center justify:center
                text "1" size:13 color:on_color
            box fill:success radius:6 height:72 align:center justify:center
                text "2" size:13 color:on_color
            box fill:danger radius:6 height:72 align:center justify:center
                text "3" size:13 color:on_color
            box fill:warning radius:6 height:72 align:center justify:center
                text "4" size:13 color:on_color
            box fill:purple radius:6 height:72 align:center justify:center
                text "5" size:13 color:on_color
            box fill:dark radius:6 height:72 align:center justify:center
                text "6" size:13 color:on_color
        text "Explicit placement (grid_column_span)" size:12 color:muted
        grid cols:3 gap:12
            box fill:dark radius:6 height:48 span:3 align:center justify:center
                text "header — span 3" size:13 color:on_color
            box fill:success radius:6 height:72 align:center justify:center
                text "A" size:13 color:on_color
            box fill:danger radius:6 height:72 align:center justify:center
                text "B" size:13 color:on_color
        text "Nested in Container" size:12 color:muted
        row gap:16
            col width:180
                text "Grid nested" size:13 color:muted
                text "inside flex →" size:13 color:muted
            grid cols:"1fr 1fr" gap:8 grow:1
                box fill:primary radius:6 height:72 align:center justify:center
                    text "G1" size:13 color:on_color
                box fill:success radius:6 height:72 align:center justify:center
                    text "G2" size:13 color:on_color
                box fill:danger radius:6 height:72 align:center justify:center
                    text "G3" size:13 color:on_color
                box fill:warning radius:6 height:72 align:center justify:center
                    text "G4" size:13 color:on_color
