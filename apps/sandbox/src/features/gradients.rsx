[view]
col gap:20
    doc_header kicker:"06 · SURFACES" title:"Gradients" desc:"Fill a box with a linear or radial gradient between two colors — add a middle stop for a three-color blend."

    col gap:8
        text "Linear — direction sets the axis" size:13 color:ink
        card
            grid cols:"fit 160" gap:12
                col gap:6 align:center
                    box gradient:horizontal from:danger to:primary radius:10 width:100% height:72
                    text "horizontal" size:12 color:muted
                col gap:6 align:center
                    box gradient:vertical from:purple to:success radius:10 width:100% height:72
                    text "vertical" size:12 color:muted
                col gap:6 align:center
                    box gradient:diagonal from:warning to:danger radius:10 width:100% height:72
                    text "diagonal" size:12 color:muted
        code_line code:"box gradient:horizontal from:danger to:primary"

    col gap:8
        text "Three stops — add a middle color and its position" size:13 color:ink
        card
            box gradient:horizontal from:primary mid:purple mid_pos:0.5 to:danger radius:10 width:100% height:80
        code_line code:"box gradient:horizontal from:primary mid:purple mid_pos:0.5 to:danger"

    col gap:8
        text "Radial — a burst from the center; gr sets the radius" size:13 color:ink
        card
            grid cols:"fit 160" gap:12
                col gap:6 align:center
                    box gradient:radial gr:70 from:cyan to:primary radius:10 width:100% height:80
                    text "gr:70" size:12 color:muted
                col gap:6 align:center
                    box gradient:radial from:warning to:danger radius:10 width:100% height:80
                    text "default radius" size:12 color:muted
                col gap:6 align:center
                    box gradient:radial gr:80 from:success mid:cyan mid_pos:0.45 to:purple radius:10 width:100% height:80
                    text "3 stops" size:12 color:muted
        code_line code:"box gradient:radial gr:70 from:cyan to:primary"

    col gap:8
        text "Attributes" size:13 color:ink
        col gap:6
            prop_row name:"gradient" values:"horizontal·vertical·diagonal·radial" about:"Gradient kind and axis."
            prop_row name:"from / to" values:"token · #hex" about:"Start and end colors (required)."
            prop_row name:"mid / mid_pos" values:"color · 0–1" about:"Optional middle stop and its position."
            prop_row name:"gr" values:"number" about:"Radial radius in px (default: half the shorter side)."
