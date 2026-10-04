[logic]
use crate::shared::components::card::{card, CardProps};
use crate::shared::components::code_line::{code_line, CodeLineProps};
use crate::shared::components::doc_header::{doc_header, DocHeaderProps};
use crate::shared::components::example::{example, ExampleProps};
use crate::shared::components::prop_row::{prop_row, PropRowProps};

let hovered = signal(false);

[view]
col gap:20
    doc_header kicker:"FOUNDATIONS" title:"Case & underline" desc:"text_case shows a text in capitals, small letters or capitalized words without rewriting it: every target measures the string it shows, and a reader still reads what was written. underline draws a line under the glyphs, placed from the font unless its offset, thickness and colour say otherwise."
    example title:"Case — written once in the catalogue, shown as the design asks"
        card gap:6
            text "work · about · contact" font_size:14 color:$theme.ink text_case:upper letter_spacing:0.08em
            text "WHEN IT SHOUTS, LOWER IT" font_size:14 color:$theme.ink text_case:lower
            text "every word starts big" font_size:14 color:$theme.ink text_case:capitalize
            text "istanbul, izmir" font_size:14 color:$theme.ink text_case:upper lang:"tr"
        code_line code:"text t!(\"nav.work\") text_case:upper   ·   lang:\"tr\" dots the capital İ"
    example title:"Underline — from the font, offset, and on hover"
        card gap:10
            text "From the font's own metrics" font_size:16 color:$theme.ink underline
            text "Offset 4 · 2 thick · accent" font_size:16 color:$theme.ink underline underline_offset:4 underline_thickness:2 underline_color:$theme.primary
            text "Offset in em: 0.18em" font_size:24 color:$theme.ink underline underline_offset:0.18em underline_thickness:2
            box on_hover:(|h| $hovered.set(h)) cursor:pointer
                text "Hover me to underline" font_size:16 color:$theme.primary underline:$hovered underline_offset:4
            text "A paragraph with " font_size:14 color:$theme.ink
                span "one underlined run" underline underline_color:$theme.danger
                span " inside it."
        code_line code:"text 'Contact' underline underline_offset:0.18em underline_thickness:2 underline_color:$theme.primary"
    example title:"On each target"
        col gap:8
            text "Web (document) writes text-transform and the text-decoration longhands, so the page keeps the written text; the canvas, GPU and software targets case the string before shaping and draw the line beneath the glyphs, snapped to the pixel grid; the terminal shows the cased string and underlines the cells, with no offset, thickness or colour of its own. A field (input) always shows the case it was typed in." font_size:13 color:$theme.muted
    example title:"Attributes"
        col gap:6
            prop_row name:"text_case" values:"upper · lower · capitalize · none" about:"The case the glyphs show; inherited, so a container can set it for its whole subtree."
            prop_row name:"underline" values:"flag · true · false · $state" about:"Draws the line; reading state, it follows a hover or a selection."
            prop_row name:"underline_offset" values:"px · em · sw/sh" about:"From the baseline down to the line's top edge, as text-underline-offset."
            prop_row name:"underline_thickness" values:"px · em · sw/sh" about:"How thick the line is; never thinner than a device pixel."
            prop_row name:"underline_color" values:"color" about:"The line's colour; the text's own when unset."
