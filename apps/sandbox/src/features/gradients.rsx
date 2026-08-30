[logic]
use crate::shared::components::card::{card, CardProps};
use crate::shared::components::code_line::{code_line, CodeLineProps};
use crate::shared::components::doc_header::{doc_header, DocHeaderProps};
use crate::shared::components::example::{example, ExampleProps};
use crate::shared::components::prop_row::{prop_row, PropRowProps};

[style]
@swatch
    gap: 6
    align: center

[view]
col gap:20
    doc_header kicker:"SURFACES" title:"Gradients" desc:"Fill a box with a linear or radial gradient: name the stops in order, and where any of them sits."
    example title:"Linear — the direction leads, or the run goes top to bottom"
        card
            grid cols:"fit 160" gap:12
                col @swatch
                    box fill:linear(horizontal, $theme.danger, $theme.primary) radius:10 width:100% height:72
                    text "horizontal" font_size:12 color:$theme.muted
                col @swatch
                    box fill:linear($theme.purple, $theme.success) radius:10 width:100% height:72
                    text "vertical (the default)" font_size:12 color:$theme.muted
                col @swatch
                    box fill:linear(diagonal, $theme.warning, $theme.danger) radius:10 width:100% height:72
                    text "diagonal" font_size:12 color:$theme.muted
        code_line code:"box fill:linear(horizontal, $theme.danger, $theme.primary)"
    example title:"Three stops — a stop with no position of its own takes an even share"
        card
            box fill:linear(horizontal, $theme.primary, $theme.purple, $theme.danger) radius:10 width:100% height:80
        code_line code:"box fill:linear(horizontal, $theme.primary, $theme.purple, $theme.danger)"
    example title:"Radial — a burst from the center; a leading number sets the radius"
        card
            grid cols:"fit 160" gap:12
                col @swatch
                    box fill:radial(70, $theme.cyan, $theme.primary) radius:10 width:100% height:80
                    text "radius 70" font_size:12 color:$theme.muted
                col @swatch
                    box fill:radial($theme.warning, $theme.danger) radius:10 width:100% height:80
                    text "default radius" font_size:12 color:$theme.muted
                col @swatch
                    box fill:radial(80, $theme.success, $theme.cyan 0.45, $theme.purple) radius:10 width:100% height:80
                    text "3 stops, the middle at 0.45" font_size:12 color:$theme.muted
        code_line code:"box fill:radial(70, $theme.cyan, $theme.primary)"
    example title:"Attributes"
        col gap:6
            prop_row name:"linear(…)" values:"[horizontal·vertical·diagonal,] stop, stop…" about:"A run between two or more stops. Vertical unless the axis leads."
            prop_row name:"radial(…)" values:"[radius,] stop, stop…" about:"A burst from the center. Half the shorter side unless a radius leads."
            prop_row name:"stop" values:"color [position]" about:"A color, and where it sits from 0 to 1. Unpositioned stops spread evenly."
