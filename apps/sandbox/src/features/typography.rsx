[view]
col gap:20
    doc_header kicker:"03 · FOUNDATIONS" title:"Typography" desc:"Text takes a size and a color token, measures its own height, and wraps to the available width automatically."

    col gap:8
        text "Size scale" size:13 color:ink
        box fill:surface stroke:border radius:12 pad:16 gap:6
            text "Display · 32" size:32 color:ink
            text "Title · 24" size:24 color:ink
            text "Heading · 18" size:18 color:ink
            text "Body · 14 — the quick brown fox jumps over the lazy dog" size:14 color:ink
            text "Caption · 12" size:12 color:muted
        code_line code:"text 'Heading' size:18 color:ink"

    col gap:8
        text "Color tokens applied to text" size:13 color:ink
        box fill:surface stroke:border radius:12 pad:16 gap:4
            text "ink — primary reading color" size:14 color:ink
            text "muted — secondary and captions" size:14 color:muted
            text "primary — links and emphasis" size:14 color:primary
            text "success · danger · warning" size:14 color:success
        code_line code:"text 'muted' color:muted"

    col gap:8
        text "Wrapping — a paragraph measures its own height at any width" size:13 color:ink
        box fill:surface stroke:border radius:12 pad:16
            text "Text nodes wrap to the width they are given and report the exact height the wrapped lines need, so the sibling below them is never overlapped — resize the window and watch this paragraph reflow while the box grows to fit it." size:14 color:muted max_width:520
        code_line code:"text '…long copy…' color:muted max_width:520"

    col gap:8
        text "Interpolation — embed values with {{ }} (see the Reactivity section)" size:13 color:ink
        box fill:surface stroke:border radius:12 pad:16
            text "Braces splice a signal or expression straight into the string." size:13 color:muted
        code_line code:"text 'Count: {$count}'      text '{props.title}'"

    col gap:8
        text "Attributes" size:13 color:ink
        col gap:6
            prop_row name:"size" values:"number" about:"Font size in px (default 14)."
            prop_row name:"color" values:"token · #hex · $signal" about:"Text color (default ink via a token)."
            prop_row name:"max_width" values:"number" about:"Wrap boundary for long copy."
            prop_row name:"height" values:"number" about:"Pin the box instead of auto-measuring."
