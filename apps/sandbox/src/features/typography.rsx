[logic]
use crate::shared::components::card::{card, CardProps};
use crate::shared::components::code_line::{code_line, CodeLineProps};
use crate::shared::components::doc_header::{doc_header, DocHeaderProps};
use crate::shared::components::example::{example, ExampleProps};
use crate::shared::components::prop_row::{prop_row, PropRowProps};

[view]
col gap:20
    doc_header kicker:"FOUNDATIONS" title:"Typography" desc:"Text takes a size and a color token, measures its own height, and wraps to the available width automatically."
    example title:"Size scale"
        card gap:6
            text "Display · 32" font_size:32 color:$theme.ink
            text "Title · 24" font_size:24 color:$theme.ink
            text "Heading · 18" font_size:18 color:$theme.ink
            text "Body · 14 — the quick brown fox jumps over the lazy dog" font_size:14 color:$theme.ink
            text "Caption · 12" font_size:12 color:$theme.muted
        code_line code:"text 'Heading' font_size:18 color:$theme.ink"
    example title:"Color tokens applied to text"
        card gap:4
            text "ink — primary reading color" font_size:14 color:$theme.ink
            text "muted — secondary and captions" font_size:14 color:$theme.muted
            text "primary — links and emphasis" font_size:14 color:$theme.primary
            text "success · danger · warning" font_size:14 color:$theme.success
        code_line code:"text 'muted' color:$theme.muted"
    example title:"Weight, italic & alignment"
        card gap:6
            text "Light — weight:300" font_size:16 color:$theme.ink font_weight:300
            text "Semibold — weight:600" font_size:16 color:$theme.ink font_weight:600
            text "Bold — weight:bold" font_size:16 color:$theme.ink font_weight:bold
            text "Italic emphasis" font_size:16 color:$theme.ink font_style:italic
            text "Bold italic" font_size:16 color:$theme.ink font_weight:bold font_style:italic
            text "Centered in its box" font_size:14 color:$theme.muted text_align:center
            text "Aligned to the end" font_size:14 color:$theme.muted text_align:right
        code_line code:"text 'Bold' font_weight:bold   ·   'Note' font_style:italic   ·   '…' text_align:center"
    example title:"Line clamp & ellipsis"
        card gap:8
            text "This paragraph is clamped to two lines with lines:2, so however long the copy gets the box never grows past two lines and the overflow is simply dropped." font_size:14 color:$theme.muted lines:2
            text "With ellipsis the truncated tail is replaced by a … so it reads as intentionally cut rather than abruptly clipped at the boundary." font_size:14 color:$theme.ink lines:2 ellipsis
            text "A single-line label that ellipsizes when it runs out of room in its box." font_size:14 color:$theme.primary lines:1 ellipsis max_width:300
        code_line code:"text '…' lines:2 ellipsis   ·   'label' lines:1 ellipsis"
    example title:"Wrapping — a paragraph measures its own height at any width"
        card
            text "Text nodes wrap to the width they are given and report the exact height the wrapped lines need, so the sibling below them is never overlapped — resize the window and watch this paragraph reflow while the box grows to fit it." font_size:14 color:$theme.muted max_width:520
        code_line code:"text '…long copy…' color:$theme.muted max_width:520"
    example title:"Interpolation — embed values with { } (see the Reactivity section)"
        card
            text "Braces splice a signal or expression straight into the string." font_size:13 color:$theme.muted
        code_line code:"text 'Count: {$count}'      text '{props.title}'"
    example title:"heading and section — an accent title, alone or above its content"
        card gap:12
            heading text:"A heading is a real title"
            section title:"A section wraps a heading above its own content"
                text "The heading sits above these children in a small-gap column." font_size:13 color:$theme.muted
                text "Use it to group a labelled block without hand-building the column." font_size:13 color:$theme.muted
        code_line code:"heading 'Title'      section 'Title' > …children…"
    example title:"t! — a catalogue lookup is Rust, so it goes where any value goes"
        card gap:10
            row gap:12 align:center
                button label:t!("nav.overview") ghost
                text "{t!(\"greeting\", name = \"Ada\")}" font_size:13 color:$theme.muted
            text "The macro validates the key against locales/ at compile time and re-reads the locale, so a language switch re-renders both." font_size:12 color:$theme.muted
        code_line code:"button label:t!('nav.overview')   ·   text '{t!(\'greeting\', name = n)}'"
    example title:"Attributes"
        col gap:6
            prop_row name:"font_size" values:"number" about:"Font size in px (default 14)."
            prop_row name:"color" values:"token · #hex · $signal" about:"Text color (default ink via a token)."
            prop_row name:"font_weight" values:"thin…black · 100–900" about:"Font weight, keyword or number (default 400)."
            prop_row name:"font_style" values:"normal·italic·oblique" about:"Slant the text."
            prop_row name:"text_align" values:"left·center·right·justify" about:"Horizontal alignment within the box."
            prop_row name:"lines" values:"number" about:"Clamp to at most N lines (extra dropped)."
            prop_row name:"ellipsis" values:"flag" about:"Replace the clamped tail with a … ."
            prop_row name:"max_width" values:"number" about:"Wrap boundary for long copy."
            prop_row name:"height" values:"number" about:"Pin the box instead of auto-measuring."
