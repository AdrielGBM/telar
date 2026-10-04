[logic]
use crate::shared::components::card::{card, CardProps};
use crate::shared::components::code_line::{code_line, CodeLineProps};
use crate::shared::components::doc_header::{doc_header, DocHeaderProps};
use crate::shared::components::example::{example, ExampleProps};
use crate::shared::components::prop_row::{prop_row, PropRowProps};

let frosted = signal(true);
let blur = move || if frosted.get() { 20.0f32 } else { 0.0 };

[style]
@stripe
    width: 28
    height: 160

[view]
col gap:20
    doc_header kicker:"SURFACES" title:"Backdrop blur" desc:"backdrop_blur blurs what the surface drew beneath a box before the box paints, so a translucent fill reads as frosted glass over whatever scrolls behind it — a header over a page, a panel over a picture."
    example title:"A frosted panel over stripes"
        card pad:24
            box height:160
                row
                    box @stripe fill:$theme.primary
                    box @stripe fill:$theme.warning
                    box @stripe fill:$theme.danger
                    box @stripe fill:$theme.purple
                    box @stripe fill:$theme.success
                    box @stripe fill:$theme.primary
                    box @stripe fill:$theme.warning
                    box @stripe fill:$theme.danger
                    box @stripe fill:$theme.purple
                    box @stripe fill:$theme.success
                box absolute inset_top:30 inset_start:40 width:220 height:100 radius:14 fill:#ffffff59 backdrop_blur:blur() transition(backdrop_blur 250ms) on_press:(|| $frosted.set(!$frosted.get())) cursor:pointer align:center justify:center
                    text "Tap to toggle the blur" font_size:14 color:#111111
        code_line code:"box fill:#ffffff59 backdrop_blur:20   (CSS backdrop-filter: blur(10px))"
    example title:"On each target"
        col gap:8
            text "The value is a radius, as shadow_blur is, so CSS's blur(10px) is backdrop_blur:20. The document writes backdrop-filter; GPU and software blur what the surface drew beneath the box, inside its rect. The terminal has no backdrop to blur and shows the translucent fill alone." font_size:13 color:$theme.muted
    example title:"Attributes"
        col gap:6
            prop_row name:"backdrop_blur" values:"px · $state" about:"Blurs what lies beneath the box; animates with transition(backdrop_blur …)."
