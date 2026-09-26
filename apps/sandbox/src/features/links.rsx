[logic]
use crate::shared::components::card::{card, CardProps};
use crate::shared::components::code_line::{code_line, CodeLineProps};
use crate::shared::components::doc_header::{doc_header, DocHeaderProps};
use crate::shared::components::example::{example, ExampleProps};

let page = signal(1u32);

[view]
col gap:20
    doc_header kicker:"NAVIGATION" title:"Links" desc:"to: makes a box a link: somewhere in the app, somewhere on the page, or somewhere outside it. It joins the tab order, follows on a tap or Enter, and is announced with where it goes. Each target goes there its own way: a real <a href> in a document, a new tab from a canvas, the system browser on the desktop, an ACTION_VIEW intent on Android, OSC 8 in a terminal."
    example title:"A route in the app"
        card gap:10
            row gap:10 align:center
                box to:Location::root().segment("links").segment($page.to_string()) fill:$theme.surface_alt radius:8 pad_x:14 pad_y:6 hover_style(fill:$theme.border)
                    text "Open /links/{$page}" font_size:14 color:$theme.primary
                box fill:$theme.surface_alt radius:8 pad_x:14 pad_y:6 hover_style(fill:$theme.border) on_press:(|| $page.set($page.get() + 1))
                    text "Next page" font_size:14 color:$theme.ink
            text "A plain press pushes the route onto the app's history. Ctrl, Cmd or Shift open it beside this one where there is a beside: a browser tab." font_size:12 color:$theme.muted
        code_line code:"box to:Page::Project(slug)   ·   any typed Route, or a Location"
    example title:"Outside the app"
        card gap:10
            box to:external("https://github.com/AdrielGBM/telar") fill:$theme.surface_alt radius:8 pad_x:14 pad_y:6 hover_style(fill:$theme.border)
                text "Telar on GitHub" font_size:14 color:$theme.primary
            box to:external("mailto:someone@example.com") fill:$theme.surface_alt radius:8 pad_x:14 pad_y:6 hover_style(fill:$theme.border)
                text "Write an email" font_size:14 color:$theme.primary
        code_line code:"box to:external(\"https://…\")   ·   open_uri: the portal or xdg-open, ShellExecute, NSWorkspace, window.open"
    example title:"On this page"
        card gap:10
            box to:anchor("top") fill:$theme.surface_alt radius:8 pad_x:14 pad_y:6 hover_style(fill:$theme.border)
                text "Back to the top" font_size:14 color:$theme.primary
            text "An anchor is revealed once a box names itself with anchor:, which is still to come; until then following one does nothing." font_size:12 color:$theme.muted
        code_line code:"box to:anchor(\"top\")"
