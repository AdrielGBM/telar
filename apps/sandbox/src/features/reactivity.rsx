[logic]
let count = signal(0i32);
let doubled = memo(move || count.get() * 2);
let remaining = memo(move || (10 - count.get()).max(0));
// A memo of the fill level, read reactively by the bar's opacity below.
let level = memo(move || (count.get() as f32 / 10.0).clamp(0.18, 1.0));

[view]
col gap:20
    doc_header kicker:"14 · INTERACTION" title:"Reactivity" desc:"A signal is reactive state; a memo derives from it. Read one with {$signal} and only the widgets that touch it recompute — no virtual DOM, no diffing."

    col gap:8
        text "One signal, several derived readers" size:13 color:ink
        box fill:surface stroke:border radius:12 pad:16 gap:8
            text "count · {$count}" size:22 color:ink
            text "doubled memo · {$doubled}" size:14 color:muted
            text "{$remaining} left before 10" size:14 color:primary
            row gap:10
                btn "−" outline:primary on_press:|| $count.update(|n| *n = (*n - 1).max(0))
                btn "+" fill:primary on_press:|| $count.update(|n| *n = (*n + 1).min(10))
                btn "Reset" ghost on_press:|| $count.set(0)
        code_line code:"let count = signal(0);   let doubled = memo(move || count.get() * 2);"

    col gap:8
        text "A reactive property — the bar's opacity tracks the same signal" size:13 color:ink
        box fill:surface stroke:border radius:12 pad:16 gap:8
            box fill:primary radius:8 height:36 opacity:$level align:center justify:center
                text "opacity = count / 10" size:13 color:on_primary
            text "Press + and this bar fades in — a signal wired straight into a style." size:12 color:muted
        code_line code:"box fill:primary opacity:$level      (level is a memo of count/10)"

    col gap:8
        text "Primitives" size:13 color:ink
        col gap:6
            prop_row name:"signal(v)" values:"RwSignal<T>" about:"Reactive state. .get() .set() .update() .peek()."
            prop_row name:"memo(f)" values:"Memo<T>" about:"Cached value that recomputes when its deps change."
            prop_row name:"{$name}" values:"interpolation" about:"Read a signal or memo inside a string."
            prop_row name:"$name" values:"in closures" about:"The handle itself, for .set / .update."
