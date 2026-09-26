[logic]
use crate::shared::components::card::{card, CardProps};
use crate::shared::components::code_line::{code_line, CodeLineProps};
use crate::shared::components::doc_header::{doc_header, DocHeaderProps};
use crate::shared::components::example::{example, ExampleProps};
use crate::shared::components::prop_row::{prop_row, PropRowProps};

[style]
@blend_center
    align: center
    justify: center

@blend_swatch
    width: 96
    height: 96
    radius: 10

[view]
col gap:20
    doc_header kicker:"SURFACES" title:"Blend modes" desc:"blend composites a box and everything inside it onto what is beneath it through a mode, the same way CSS mix-blend-mode does: a texture multiplies or screens over a wallpaper instead of covering it plainly."
    example title:"A tinted disc over a striped backdrop, at each mode"
        card pad:24
            row gap:16 wrap
                box @blend_center @blend_swatch fill:linear(horizontal, $theme.primary, $theme.purple, $theme.danger)
                    box @blend_swatch fill:$theme.warning blend:normal
                        text "normal" font_size:12 color:$theme.ink
                box @blend_center @blend_swatch fill:linear(horizontal, $theme.primary, $theme.purple, $theme.danger)
                    box @blend_swatch fill:$theme.warning blend:multiply
                        text "multiply" font_size:12 color:$theme.ink
                box @blend_center @blend_swatch fill:linear(horizontal, $theme.primary, $theme.purple, $theme.danger)
                    box @blend_swatch fill:$theme.warning blend:screen
                        text "screen" font_size:12 color:$theme.ink
                box @blend_center @blend_swatch fill:linear(horizontal, $theme.primary, $theme.purple, $theme.danger)
                    box @blend_swatch fill:$theme.warning blend:difference
                        text "difference" font_size:12 color:$theme.on_primary
                box @blend_center @blend_swatch fill:linear(horizontal, $theme.primary, $theme.purple, $theme.danger)
                    box @blend_swatch fill:$theme.warning blend:color-dodge
                        text "color-dodge" font_size:12 color:$theme.ink
        code_line code:"box fill:$theme.warning blend:multiply"
    example title:"Isolated from what is behind the card"
        col gap:8
            text "The blended swatch above composites against its own backdrop box, not against the page: the parent that holds it gets `isolation: isolate` on web so a texture meant to multiply against its neighbor does not also ghost into content several levels up. GPU and software already render each layer through its own compositing pass, so they need no such fix; TUI has no notion of a backdrop to blend against and ignores the attribute." font_size:13 color:$theme.muted
    example title:"Attributes"
        col gap:6
            prop_row name:"blend" values:"normal · multiply · screen · overlay · darken · lighten · color-dodge · color-burn · hard-light · soft-light · difference · exclusion · hue · saturation · color · luminosity · plus" about:"Composites the box through a CSS mix-blend-mode-style mode; normal is the default and costs nothing extra."
