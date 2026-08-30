[logic]
use crate::shared::components::card::{card, CardProps};
use crate::shared::components::code_line::{code_line, CodeLineProps};
use crate::shared::components::doc_header::{doc_header, DocHeaderProps};
use crate::shared::components::example::{example, ExampleProps};
use crate::shared::components::prop_row::{prop_row, PropRowProps};
let count = signal(0i32);
let doubled = memo(move || count.get() * 2);
let remaining = memo(move || (10 - count.get()).max(0));
// A memo of the fill level, read reactively by the bar's opacity below.
let level = memo(move || (count.get() as f32 / 10.0).clamp(0.18, 1.0));

// A reactive list: `for todo in $todos key todo.id` reconciles item widgets as this Vec changes.
#[derive(Clone)]
struct Todo {
    id: u32,
    label: &'static str,
}
let todos = signal(vec![
    Todo {
        id: 1,
        label: "Slots & optional props",
    },
    Todo {
        id: 2,
        label: "Rich text — weight, italic, ellipsis",
    },
    Todo {
        id: 3,
        label: "Reactive lists",
    },
]);

// A reactive conditional: `if $show { … } else { … }` swaps the visible branch when this flips.
let show = signal(true);

// Drives a `lazy` block: its subtree is not built until this first turns true, and is kept from then on.
let show_panel = signal(false);
// Typed into the panel below — still there after closing and reopening it, which is the whole difference.
let panel_note = signal(String::new());
// Bumped by a `let` inside the lazy block, which runs when that subtree is CONSTRUCTED. Read only from inside the panel, so it counts builds of exactly that subtree: it must reach 1 and stay there.
let panel_builds = signal(0i32);

// Text signals driven by `input` primitives (Tab moves focus between the two fields).
let name = signal(String::new());
let email = signal(String::new());
// Whether the focusable box below currently holds focus (driven by its on_focus callback).
let box_focused = signal(false);

// Toggles a modal rendered in an `overlay` (a top-layer portal).
let show_modal = signal(false);

// The drag primitive: a 0..1 value with a derived thumb offset (px) and percentage.
let slider = signal(0.4f32);
let thumb_x = memo(move || slider.get() * 266.0);
let pct = memo(move || (slider.get() * 100.0) as i32);

[view]
col gap:20
    doc_header kicker:"INTERACTION" title:"Reactivity" desc:"A signal is reactive state; a memo derives from it. Read one with {$signal} and only the widgets that touch it recompute — no virtual DOM, no diffing."
    example title:"One signal, several derived readers"
        card gap:8
            text "count · {$count}" font_size:22 color:theme.ink
            text "doubled memo · {$doubled}" font_size:14 color:theme.muted
            text "{$remaining} left before 10" font_size:14 color:theme.primary
            row gap:10
                button label:"−" outline:theme.primary on_press:(|| $count.update(|n| *n = (*n - 1).max(0)))
                button label:"+" fill:theme.primary on_press:(|| $count.update(|n| *n = (*n + 1).min(10)))
                button label:"Reset" ghost on_press:(|| $count.set(0))
        code_line code:"let count = signal(0);   let doubled = memo(move || count.get() * 2);"
    example title:"A reactive property — the bar's opacity tracks the same signal"
        card gap:8
            box fill:theme.primary radius:8 height:36 opacity:$level align:center justify:center
                text "opacity = count / 10" font_size:13 color:theme.on_primary
            text "Press + and this bar fades in — a signal wired straight into a style." font_size:12 color:theme.muted
        code_line code:"box fill:theme.primary opacity:$level      (level is a memo of count/10)"
    example title:"Reactive list — for over a signal, keyed and reconciled (not rebuilt)"
        card gap:10
            row gap:8
                button label:"Add" fill:theme.primary on_press:(|| $todos.update(|v| { let id = v.iter().map(|t| t.id).max().unwrap_or(0) + 1; v.push(Todo { id, label: "New task" }); }))
                button label:"Reverse" outline:theme.primary on_press:(|| $todos.update(|v| v.reverse()))
                button label:"Remove first" ghost on_press:(|| $todos.update(|v| { if !v.is_empty() { v.remove(0); } }))
            for todo in $todos key todo.id
                row gap:8 align:center pad_y:4
                    box fill:theme.primary radius:4 width:8 height:8
                    text "{todo.label}" font_size:14 color:theme.ink
        code_line code:"for todo in $todos key todo.id   >   row …   (reused/moved/dropped by key)"
    example title:"Reactive if/else — the shown branch swaps on a signal"
        card gap:10
            button label:"Toggle" fill:theme.primary on_press:(|| $show.toggle())
            if $show
                row gap:8 align:center pad_y:4
                    box fill:theme.primary radius:4 width:8 height:8
                    text "Now you see me — and the count above is {$count}" font_size:14 color:theme.ink
            else
                text "…now you don't" font_size:14 color:theme.muted
        code_line code:"if $show  >  …  else  …   (branch swaps; old nodes disposed, new built)"
    example title:"lazy — a subtree built on its first display, then only shown and hidden"
        card gap:10
            text "This panel costs nothing until you open it the first time. Type into it, close it, open it again: the text is still there. An if would have disposed the whole branch and rebuilt it empty." font_size:13 color:theme.muted
            button label:"Toggle panel" fill:theme.primary on_press:(|| $show_panel.toggle())
            lazy when:$show_panel
                let _ = panel_builds.update(|n| *n += 1)
                col gap:8
                    text "this subtree has been built {$panel_builds} time(s) — reopening never adds one" font_size:13 color:theme.primary
                    box fill:theme.surface_alt stroke:theme.border radius:8 pad_x:theme.gutter pad_y:12 width:300
                        input value:$panel_note font_size:15 color:theme.ink
        code_line code:"lazy when:$open  >  …   (deferred until first shown, then kept)"
    example title:"input + focus — two editable fields; Tab moves focus, each binds a signal (wrap a box for the look)"
        card gap:10
            box fill:theme.surface_alt stroke:theme.border radius:8 pad_x:12 pad_y:10 width:300
                input value:$name font_size:15 color:theme.ink
            box fill:theme.surface_alt stroke:theme.border radius:8 pad_x:12 pad_y:10 width:300
                input value:$email font_size:15 color:theme.ink
            text "Hello, {$name}! ({$email})" font_size:14 color:theme.muted
        code_line code:"input value:$name      (tap or Tab to focus · type · ← → Home End ⌫ · Esc blurs)"
    example title:"on_focus — any box can be focusable: it joins the Tab order and observes its own focus"
        card gap:10
            box fill:theme.surface_alt radius:8 pad:16 on_focus:(|f| $box_focused.set(f))
                text "Tab to me — focused: {$box_focused}" font_size:14 color:theme.ink
        code_line code:"box on_focus(|f| $box_focused.set(f))      (Tab-focusable; drive a focus ring)"
    example title:"on_drag — the drag base primitive: press-and-move reports the pointer, mapped to a value"
        card gap:10
            box fill:theme.surface_alt radius:7 height:14 width:280 on_drag:(|px, _py| $slider.set((px / 280.0).clamp(0.0, 1.0)))
                box fill:theme.primary radius:7 width:14 height:14 translate_x:$thumb_x
            text "value · {$pct}%" font_size:14 color:theme.muted
        code_line code:"box on_drag(|px, _| $slider.set(px / 280))      (keeps tracking even off the track)"
    example title:"overlay — a top-layer portal (modal/dropdown/toast): draws above everything, escapes clipping"
        card gap:10
            button label:"Open modal" fill:theme.primary on_press:(|| $show_modal.set(true))
        if $show_modal
            overlay
                box fill:#00000080 grow:1 align:center justify:center on_press:(|| $show_modal.set(false))
                    box fill:theme.surface stroke:theme.border radius:12 pad:24 gap:12 on_press:(|| ())
                        text "I'm rendered in an overlay" font_size:18 color:theme.ink
                        text "Above the page and outside any clip. Click the dim area to dismiss." font_size:13 color:theme.muted
                        button label:"Close" outline:theme.primary on_press:(|| $show_modal.set(false))
        code_line code:"if $open  >  overlay  >  box (scrim, on_press dismiss)  >  box (dialog)"
    example title:"Primitives"
        col gap:6
            prop_row name:"signal(v)" values:"RwSignal<T>" about:"Reactive state. .get() .set() .update() .peek()."
            prop_row name:"memo(f)" values:"Memo<T>" about:"Cached value that recomputes when its deps change."
            prop_row name:"{$name}" values:"interpolation" about:"Read a signal or memo inside a string."
            prop_row name:"$name" values:"in closures" about:"The handle itself, for .set / .update."
