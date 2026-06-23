[view]
section "Gradients"
    col gap:16
        text "Linear" size:11 color:muted
        row gap:16
            col gap:4 align:center
                box gradient:horizontal from:danger to:primary radius:8 width:168 height:80
                text "horizontal" size:11 color:muted
            col gap:4 align:center
                box gradient:vertical from:purple to:success radius:8 width:168 height:80
                text "vertical" size:11 color:muted
            col gap:4 align:center
                box gradient:diagonal from:warning to:dark radius:8 width:168 height:80
                text "diagonal" size:11 color:muted
            col gap:4 align:center
                box gradient:horizontal from:dark mid:cyan to:white radius:8 width:168 height:80
                text "3 stops" size:11 color:muted
        text "Radial" size:11 color:muted
        row gap:16
            col gap:4 align:center
                box gradient:radial gr:70 from:primary to:transparent radius:8 width:168 height:80
                text "center burst" size:11 color:muted
            col gap:4 align:center
                box gradient:radial from:danger to:warning radius:8 width:168 height:80
                text "tight radius" size:11 color:muted
            col gap:4 align:center
                box gradient:radial gr:80 from:white mid:purple mid-pos:0.45 to:dark radius:8 width:168 height:80
                text "3 stops" size:11 color:muted
