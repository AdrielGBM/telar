[logic]
use crate::shared::components::card::{card, CardProps};
use crate::shared::components::code_line::{code_line, CodeLineProps};
use crate::shared::components::doc_header::{doc_header, DocHeaderProps};
use crate::shared::components::example::{example, ExampleProps};
use crate::shared::components::prop_row::{prop_row, PropRowProps};

[style]
@center
    align: center
    justify: center

@tile
    fill: theme.surface_alt
    stroke: theme.border
    radius: 10

[view]
col gap:20
    doc_header kicker:"SURFACES" title:"Boxes & borders" desc:"A box is a styled container: give it a fill, a stroke, and a corner radius. Those same paint attributes work on any row or col too."
    example title:"Fill, stroke, and both"
        card
            row gap:14 wrap
                box @center fill:theme.primary radius:10 width:150 height:70
                    text "fill" font_size:13 color:theme.on_primary text_align:center
                box @center stroke:theme.primary stroke_width:2 radius:10 width:150 height:70
                    text "stroke" font_size:13 color:theme.primary text_align:center
                box @center fill:theme.surface_alt stroke:theme.primary stroke_width:2 radius:10 width:150 height:70
                    text "fill + stroke" font_size:13 color:theme.ink text_align:center
        code_line code:"box fill:theme.primary radius:10      box stroke:theme.primary stroke_width:2"
    example title:"Corner radius — from sharp to a full pill"
        card
            row gap:14 wrap align:center font_size:13 color:theme.on_primary text_align:center
                box @center fill:theme.purple radius:0 width:110 height:64
                    text "0"
                box @center fill:theme.purple radius:8 width:110 height:64
                    text "8"
                box @center fill:theme.purple radius:20 width:110 height:64
                    text "20"
                box @center fill:theme.purple radius:40 width:130 height:56
                    text "pill"
        code_line code:"box radius:0   ·   radius:8   ·   radius:20   ·   radius:40"
    example title:"Stroke width — a plain box makes a hairline or a heavy border"
        card
            row gap:14 wrap align:center
                box stroke:theme.success stroke_width:1 radius:8 width:120 height:56
                box stroke:theme.success stroke_width:2 radius:8 width:120 height:56
                box stroke:theme.success stroke_width:4 radius:8 width:120 height:56
        code_line code:"box stroke:theme.success stroke_width:4"
    example title:"One side at a time — a rule, a divider, a seam"
        card
            row gap:14 wrap align:center
                box @center fill:theme.surface_alt stroke:theme.success stroke_width:"0 0 2 0" width:150 height:70
                    text "bottom" font_size:13 color:theme.ink text_align:center
                box @center fill:theme.surface_alt stroke:theme.success stroke_end:2 width:150 height:70
                    text "end" font_size:13 color:theme.ink text_align:center
                box @center fill:theme.surface_alt stroke:theme.success stroke_width:"3 0 1 0" width:150 height:70
                    text "3 top · 1 bottom" font_size:13 color:theme.ink text_align:center
        code_line code:"box stroke:theme.success stroke_width:'0 0 2 0'   ·   stroke_end:2   (start/end follow RTL)"
    example title:"Corners one at a time — a panel that meets an edge"
        card
            row gap:14 wrap align:center
                box @center fill:theme.purple radius:"16 16 0 0" width:130 height:64
                    text "top only" font_size:13 color:theme.on_primary text_align:center
                box @center fill:theme.purple radius:16 radius_bottom:0 width:130 height:64
                    text "radius_bottom:0" font_size:13 color:theme.on_primary text_align:center
                box @center fill:theme.purple radius_start:20 width:130 height:64
                    text "start" font_size:13 color:theme.on_primary text_align:center
        code_line code:"box radius:'16 16 0 0'   ·   radius:16 radius_bottom:0   ·   radius_start:20"
    example title:"Content alignment inside a box"
        card
            box @center fill:theme.surface_alt radius:10 width:100% height:96
                text "align:center justify:center" font_size:13 color:theme.ink text_align:center
        code_line code:"box align:center justify:center   (a box is a flex column)"
    example title:"Class composition — a layout class and a paint recipe on one element"
        card
            row gap:14 wrap
                box @center @tile width:150 height:70
                    text "@center @tile" font_size:13 color:theme.ink text_align:center
        code_line code:"box @center @tile   ([style] classes compose: last wins, inline still overrides)"
    example title:"Attributes"
        col gap:6
            prop_row name:"fill" values:"token · #hex · $signal" about:"Solid background color."
            prop_row name:"stroke" values:"token · #hex" about:"Border color (pair with stroke_width)."
            prop_row name:"stroke_width" values:"number · \"t r b l\"" about:"Border thickness: one value for all four sides, or the CSS shorthand for one per side."
            prop_row name:"stroke_*" values:"number" about:"One side: top · right · bottom · left · x · y, plus start/end, which follow the writing direction."
            prop_row name:"radius" values:"number · \"tl tr br bl\"" about:"Corner radius: one value for all four corners, or the CSS shorthand."
            prop_row name:"radius_*" values:"number" about:"One edge's corners: top · bottom · left · right · start · end, or one corner: top_left · top_right · bottom_right · bottom_left."
            prop_row name:"opacity" values:"0–1 · $signal" about:"Fades the box and its children as a layer."
