[view]
section "Lines"
    row gap:32 align:start
        col gap:8 grow:1
            text "Width" size:11 color:muted
            row gap:8 align:center
                text "1 px" size:11 color:muted width:56
                box fill:primary height:1 grow:1
            row gap:8 align:center
                text "2 px" size:11 color:muted width:56
                box fill:primary height:2 grow:1
            row gap:8 align:center
                text "4 px" size:11 color:muted width:56
                box fill:primary height:4 grow:1
            row gap:8 align:center
                text "8 px" size:11 color:muted width:56
                box fill:primary height:8 grow:1
            row gap:8 align:center
                text "16 px" size:11 color:muted width:56
                box fill:primary height:16 grow:1
        col gap:8 grow:1
            text "Color" size:11 color:muted
            row gap:8 align:center
                box fill:primary height:3 grow:1
                text "primary" size:11 color:primary width:56
            row gap:8 align:center
                box fill:success height:3 grow:1
                text "success" size:11 color:success width:56
            row gap:8 align:center
                box fill:danger height:3 grow:1
                text "danger" size:11 color:danger width:56
            row gap:8 align:center
                box fill:warning height:3 grow:1
                text "warning" size:11 color:warning width:56
            row gap:8 align:center
                box fill:purple height:3 grow:1
                text "purple" size:11 color:purple width:56
    text "Separator" size:11 color:muted
    box fill:card_border height:1
