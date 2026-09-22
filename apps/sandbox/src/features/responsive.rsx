[logic]
use crate::shared::components::card::{card, CardProps};
use crate::shared::components::code_line::{code_line, CodeLineProps};
use crate::shared::components::doc_header::{doc_header, DocHeaderProps};
use crate::shared::components::example::{example, ExampleProps};
use crate::shared::components::prop_row::{prop_row, PropRowProps};

let width = memo(|| use_surface_width().round() as i32);
let height = memo(|| use_surface_height().round() as i32);

let range = breakpoint("narrow").at(640.0, "medium").at(1024.0, "wide").follow();
let gutter = breakpoint(8.0f32).at(640.0, 16.0).at(1024.0, 28.0).follow();
let tile = breakpoint(SizeDimension::Percent(1.0))
    .at(640.0, SizeDimension::Percent(0.48))
    .at(1024.0, SizeDimension::Percent(0.31))
    .follow();

[view]
col gap:20
    doc_header kicker:"FOUNDATIONS" title:"Responsive" desc:"Every surface — a window, a page, a terminal — has a size you can read, lengths that are a fraction of it, and breakpoints that choose a value by its width. Resize the window to watch them follow."
    example title:"The surface's size, read reactively"
        card gap:6
            text "{$width} × {$height}" font_size:22 color:$theme.ink
            text "range · {$range}" font_size:14 color:$theme.primary
        code_line code:"use_surface_width()   ·   use_surface_height()   ·   use_surface_size()"
    example title:"Breakpoints — chosen again only when the width crosses 640 or 1024"
        card
            box fill:$theme.surface_alt radius:8 pad:$gutter
                row wrap gap:12
                    box fill:$theme.cyan radius:6 height:40 width:$tile
                    box fill:$theme.cyan radius:6 height:40 width:$tile
                    box fill:$theme.cyan radius:6 height:40 width:$tile
        code_line code:"let gutter = breakpoint(8.0).at(640.0, 16.0).at(1024.0, 28.0).follow();   >   pad:$gutter"
    example title:"Surface units — a fraction of the surface, not of the parent"
        card gap:10
            box fill:$theme.primary radius:6 width:50sw max_width:100% height:26
            box fill:$theme.purple radius:6 width:25sw height:26
            box fill:$theme.success radius:6 width:40smin max_width:100% height:10sh
            grid cols(15sw 1fr) gap:10
                box fill:$theme.warning radius:6 height:30
                box fill:$theme.warning radius:6 height:30
        code_line code:"box width:50sw   ·   width:25sw   ·   width:40smin height:10sh"
    example title:"Attributes"
        col gap:6
            prop_row name:"N sw / N sh" values:"number + unit" about:"A share of the surface's width or height, in hundredths."
            prop_row name:"N smin / N smax" values:"number + unit" about:"A share of the surface's shorter or longer side."
            prop_row name:"cols(15sw 1fr)" values:"tracks" about:"A grid track can be a fraction of the surface too."
            prop_row name:"breakpoint(v).at(w, v)" values:"any value" about:"A value per width range; .follow() re-resolves it only across a threshold."
