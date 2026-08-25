[view]
col gap:20
    doc_header kicker:"FOUNDATIONS" title:"Layout" desc:"Everything is a flex row or column. Nest them freely, space children with gap, pad the edges, and align on either axis."
    example title:"Row and column"
        card gap:16
            row gap:10
                box fill:theme.primary radius:6 width:44 height:44
                box fill:theme.primary radius:6 width:44 height:44
                box fill:theme.primary radius:6 width:44 height:44
            col gap:8
                box fill:theme.purple radius:6 width:88 height:22
                box fill:theme.purple radius:6 width:88 height:22
        code_line code:"row gap:10"
        code_line code:"col gap:8"
    example title:"justify — distribute along the main axis"
        card gap:10
            row justify:between gap:8
                box fill:theme.success radius:6 width:40 height:28
                box fill:theme.success radius:6 width:40 height:28
                box fill:theme.success radius:6 width:40 height:28
            row justify:center gap:8
                box fill:theme.warning radius:6 width:40 height:28
                box fill:theme.warning radius:6 width:40 height:28
        code_line code:"row justify:between   ·   center · end · around · evenly"
    example title:"align — cross-axis alignment of mixed-height children"
        card gap:12
            row align:center gap:10 height:80
                box fill:theme.cyan radius:6 width:40 height:28
                box fill:theme.cyan radius:6 width:40 height:52
                box fill:theme.cyan radius:6 width:40 height:70
        code_line code:"row align:center   ·   start · end · stretch"
    example title:"grow — children share leftover space by weight"
        card
            row gap:10
                box fill:theme.primary radius:6 grow:1 height:36
                box fill:theme.purple radius:6 grow:2 height:36
                box fill:theme.danger radius:6 grow:1 height:36
        code_line code:"box grow:2   (takes twice the free space of grow:1)"
    example title:"wrap — children flow onto new lines when they run out of room"
        card
            row gap:10 wrap
                for _ in 0..5
                    box fill:theme.warning radius:6 width:120 height:32
        code_line code:"row gap:10 wrap   >   for _ in 0..5   >   box …"
    example title:"Nesting — a small card is just rows inside a column"
        card
            box fill:theme.surface_alt radius:10 pad:14 gap:8 max_width:260
                row justify:between align:center
                    text "Storage" font_size:13 color:theme.ink
                    text "78%" font_size:13 color:theme.primary
                box fill:theme.border radius:4 height:8
                text "3.1 GB of 4 GB used" font_size:11 color:theme.muted
        code_line code:"box pad:14 gap:8   >   row justify:between   >   text …"
    example title:"scroll — a bounded viewport that scrolls its overflowing content"
        card
            scroll height:160
                col gap:8 pad_x:2 width:100%
                    for i in 0..12
                        box fill:theme.surface_alt stroke:theme.border radius:8 pad_x:14 pad_y:10
                            text "Scrollable item {i}" font_size:13 color:theme.ink
        code_line code:"scroll height:160   >   col …   >   for i in 0..12   >   box …"
    example title:"Attributes"
        col gap:6
            prop_row name:"direction" values:"row · col" about:"Main axis. row/col/box set a sensible default."
            prop_row name:"gap" values:"number" about:"Space between children (also gap_x / gap_y)."
            prop_row name:"pad" values:"number" about:"Padding on all sides (also pad_x / pad_y)."
            prop_row name:"align" values:"start·center·end·stretch" about:"Cross-axis alignment of children."
            prop_row name:"justify" values:"start·center·between·around·evenly" about:"Main-axis distribution."
            prop_row name:"grow / shrink" values:"number" about:"How a child expands or contracts to fit."
            prop_row name:"wrap" values:"flag" about:"Let children flow onto multiple lines."
