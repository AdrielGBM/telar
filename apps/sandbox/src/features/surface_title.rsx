[logic]
use crate::shared::components::card::{card, CardProps};
use crate::shared::components::code_line::{code_line, CodeLineProps};
use crate::shared::components::doc_header::{doc_header, DocHeaderProps};
use crate::shared::components::example::{example, ExampleProps};
use crate::shared::components::prop_row::{prop_row, PropRowProps};

fn bar_rule(parts: &telar::TitleParts<'_>) -> String {
    match parts.page {
        Some(page) => format!("{page} | {}", parts.app),
        None => parts.app.to_string(),
    }
}

let shown = memo(telar::use_surface_title);

[view]
col gap:20
    doc_header kicker:"NAVIGATION" title:"Surface title" desc:"The title a surface shows is derived, not set: the app's own title, then the title of the page the app's address stands on, in the active locale. Change either and the window, the browser tab, the recents screen or the terminal follows."
    example title:"What this surface is called right now"
        card gap:10
            text "{$shown}" font_size:15 color:$theme.ink
            row gap:8 wrap
                button label:"Name this page" on_press:(|| telar::set_page_title(Some("Surface title".to_string())))
                button label:"No page title" ghost on_press:(|| telar::set_page_title(None))
                button label:"Rename the app" ghost on_press:(|| telar::window::set_title("Telar sandbox"))
            text "A navigator that follows the app's address names the page itself, from its current route's title." font_size:12 color:$theme.muted
        code_line code:"impl Route for Page   ·   fn title(&self) -> Option<String>   ·   Some(t!(\"credits.title\"))"
    example title:"A rule of your own"
        card gap:10
            row gap:8 wrap
                button label:"Page | App" ghost on_press:(|| telar::set_title_format(bar_rule))
                button label:"Page — App" ghost on_press:(|| telar::set_title_format(telar::compose_title))
            text "The rule runs inside the effect that derives the title, so one that translates follows the locale too." font_size:12 color:$theme.muted
        code_line code:"set_title_format(bar_rule)   ·   compose_title(parts)   ·   use_surface_title()"
    example title:"Where each target shows it"
        col gap:6
            prop_row name:"web" values:"document.title" about:"the tab, the history entry and a bookmark."
            prop_row name:"desktop" values:"window title" about:"the title bar and the task switcher."
            prop_row name:"android" values:"task description" about:"the label on the recents screen."
            prop_row name:"terminal" values:"OSC 0" about:"the terminal's tab, given back on exit."
            prop_row name:"headless" values:"HeadlessWindow::title" about:"kept, and recorded with record_titles_into for a prerender to read."
