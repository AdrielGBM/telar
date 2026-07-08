[view]
col gap:20
    doc_header kicker:"FOUNDATIONS" title:"Typography" desc:"Text takes a size and a color token, measures its own height, and wraps to the available width automatically."
    example title:"Size scale"
        card gap:6
            text "Display · 32" size:32 color:ink
            text "Title · 24" size:24 color:ink
            text "Heading · 18" size:18 color:ink
            text "Body · 14 — the quick brown fox jumps over the lazy dog" size:14 color:ink
            text "Caption · 12" size:12 color:muted
        code_line code:"text 'Heading' size:18 color:ink"
    example title:"Color tokens applied to text"
        card gap:4
            text "ink — primary reading color" size:14 color:ink
            text "muted — secondary and captions" size:14 color:muted
            text "primary — links and emphasis" size:14 color:primary
            text "success · danger · warning" size:14 color:success
        code_line code:"text 'muted' color:muted"
    example title:"Weight, italic & alignment"
        card gap:6
            text "Light — weight:300" size:16 color:ink weight:300
            text "Semibold — weight:600" size:16 color:ink weight:600
            text "Bold — weight:bold" size:16 color:ink weight:bold
            text "Italic emphasis" size:16 color:ink italic
            text "Bold italic" size:16 color:ink weight:bold italic
            text "Centered in its box" size:14 color:muted align:center
            text "Aligned to the end" size:14 color:muted align:right
        code_line code:"text 'Bold' weight:bold   ·   'Note' italic   ·   '…' align:center"
    example title:"Line clamp & ellipsis"
        card gap:8
            text "This paragraph is clamped to two lines with lines:2, so however long the copy gets the box never grows past two lines and the overflow is simply dropped." size:14 color:muted lines:2
            text "With ellipsis the truncated tail is replaced by a … so it reads as intentionally cut rather than abruptly clipped at the boundary." size:14 color:ink lines:2 ellipsis
            text "A single-line label that ellipsizes when it runs out of room in its box." size:14 color:primary lines:1 ellipsis max_width:300
        code_line code:"text '…' lines:2 ellipsis   ·   'label' lines:1 ellipsis"
    example title:"Wrapping — a paragraph measures its own height at any width"
        card
            text "Text nodes wrap to the width they are given and report the exact height the wrapped lines need, so the sibling below them is never overlapped — resize the window and watch this paragraph reflow while the box grows to fit it." size:14 color:muted max_width:520
        code_line code:"text '…long copy…' color:muted max_width:520"
    example title:"Interpolation — embed values with { } (see the Reactivity section)"
        card
            text "Braces splice a signal or expression straight into the string." size:13 color:muted
        code_line code:"text 'Count: {$count}'      text '{props.title}'"
    example title:"heading and section — an accent title, alone or above its content"
        card gap:12
            heading text:"A heading is a real title"
            section title:"A section wraps a heading above its own content"
                text "The heading sits above these children in a small-gap column." size:13 color:muted
                text "Use it to group a labelled block without hand-building the column." size:13 color:muted
        code_line code:"heading 'Title'      section 'Title' > …children…"
    example title:"Attributes"
        col gap:6
            prop_row name:"size" values:"number" about:"Font size in px (default 14)."
            prop_row name:"color" values:"token · #hex · $signal" about:"Text color (default ink via a token)."
            prop_row name:"weight" values:"thin…black · 100–900" about:"Font weight, keyword or number (default 400)."
            prop_row name:"italic" values:"flag" about:"Slant the text."
            prop_row name:"align" values:"left·center·right·justify" about:"Horizontal alignment within the box."
            prop_row name:"lines" values:"number" about:"Clamp to at most N lines (extra dropped)."
            prop_row name:"ellipsis" values:"flag" about:"Replace the clamped tail with a … ."
            prop_row name:"max_width" values:"number" about:"Wrap boundary for long copy."
            prop_row name:"height" values:"number" about:"Pin the box instead of auto-measuring."
