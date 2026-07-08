[style]
@center
    align: center
    justify: center

[view]
col gap:20
    doc_header kicker:"FOUNDATIONS" title:"Sizing & grid" desc:"Size in pixels or percentages, clamp with min/max, and reach for the grid when you need aligned columns or a responsive gallery."
    example title:"Fixed vs percentage width"
        card gap:10
            box fill:primary radius:6 width:120 height:26
            box fill:primary radius:6 width:50% height:26
            box fill:primary radius:6 width:100% height:26
        code_line code:"box width:120   ·   width:50%   ·   width:100%"
    example title:"min_width — a box refuses to shrink below its floor, then wraps"
        card
            row gap:10 wrap
                box fill:purple radius:6 min_width:200 grow:1 height:30
                box fill:purple radius:6 min_width:200 grow:1 height:30
                box fill:purple radius:6 min_width:200 grow:1 height:30
        code_line code:"box min_width:200 grow:1   (also max_width, min_height, max_height)"
    example title:"Grid — fixed column count with repeat(3, 1fr)"
        card
            grid cols:3 gap:10
                box @center fill:cyan radius:6 height:44
                    text "1" size:13 color:on_primary
                box @center fill:cyan radius:6 height:44
                    text "2" size:13 color:on_primary
                box @center fill:cyan radius:6 height:44
                    text "3" size:13 color:on_primary
                box @center fill:cyan radius:6 height:44 span:2
                    text "span 2" size:13 color:on_primary
                box @center fill:cyan radius:6 height:44
                    text "5" size:13 color:on_primary
        code_line code:"grid cols:3 gap:10      >      box span:2"
    example title:"Grid — explicit fractional tracks"
        card
            grid cols:"1fr 2fr 1fr" gap:10
                box fill:success radius:6 height:40
                box fill:success radius:6 height:40
                box fill:success radius:6 height:40
        code_line code:"grid cols:'1fr 2fr 1fr' gap:10"
    example title:"Grid — responsive tracks that reflow like wrap but keep their height"
        card
            grid cols:"fit 160" gap:10
                box fill:warning radius:6 height:40
                box fill:warning radius:6 height:40
                box fill:warning radius:6 height:40
                box fill:warning radius:6 height:40
                box fill:warning radius:6 height:40
        code_line code:"grid cols:'fit 160'   (auto-fit, minmax(160px, 1fr))"
    example title:"Attributes"
        col gap:6
            prop_row name:"width / height" values:"number · N%" about:"Pixels, or a percentage of the parent."
            prop_row name:"min_/max_width" values:"number · N%" about:"Clamp a flexible size."
            prop_row name:"cols" values:"N · tracks · fit/fill N" about:"Turn a grid on: count, explicit tracks, or responsive."
            prop_row name:"span / row_span" values:"number" about:"How many grid tracks a cell covers."
