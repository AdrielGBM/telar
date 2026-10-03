[logic]
use crate::shared::components::card::{card, CardProps};
use crate::shared::components::code_line::{code_line, CodeLineProps};
use crate::shared::components::doc_header::{doc_header, DocHeaderProps};
use crate::shared::components::example::{example, ExampleProps};

let active = memo(|| telar::use_locale().unwrap_or_else(|| "none".to_string()));
let address = memo(|| {
    let locale = telar::use_locale().unwrap_or_default();
    telar::address_of(&telar::in_locale(&locale))
});

[view]
col gap:20
    doc_header kicker:"NAVIGATION" title:"Locale in the address" desc:"follow_location_locale makes the language part of where the reader is: /es/ and /en/ on the web, the --location argument and the saved history on a desktop or a terminal, the ACTION_VIEW link on Android, the fixed location of a headless run. The address opened at decides the language; one that names none opens in the last one chosen, or the one the system prefers."
    example title:"Switch the language, keep the place"
        card gap:10
            text "{t!(\"greeting\", name = \"Ada\")}" font_size:15 color:$theme.ink
            text "locale · {$active}   ·   address · {$address}" font_size:13 color:$theme.primary
            row gap:8
                button label:"English" ghost on_press:(|| telar::set_locale("en"))
                button label:"Español" ghost on_press:(|| telar::set_locale("es"))
                button label:"العربية" ghost on_press:(|| telar::set_locale("ar"))
            text "set_locale rewrites the entry shown in the new locale: the page and the anchor stay, and no entry is added, so back still leaves the page. The choice is kept for the next run." font_size:12 color:$theme.muted
        code_line code:"follow_location_locale([\"en\", \"es\", \"ar\"], \"en\")   ·   set_locale(\"es\")"
    example title:"A link to this place in another language"
        col anchor:t!("locale_address.anchor")
            card gap:10
                row gap:10 align:center
                    box to:in_locale("en") fill:$theme.surface_alt radius:8 pad_x:14 pad_y:6 hover_style(fill:$theme.border)
                        text "Read in English" font_size:14 color:$theme.primary
                    box to:in_locale("es") fill:$theme.surface_alt radius:8 pad_x:14 pad_y:6 hover_style(fill:$theme.border)
                        text "Leer en español" font_size:14 color:$theme.primary
                text "in_locale names the place being shown in another language. A document writes it as an <a href> with the locale's prefix, so it switches before any code runs; elsewhere a tap switches in place. This example is an anchor named by t!, so its name in the address follows the language too." font_size:12 color:$theme.muted
        code_line code:"box to:in_locale(\"es\")   ·   col anchor:t!(\"locale_address.anchor\")"
