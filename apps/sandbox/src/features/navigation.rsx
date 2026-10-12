[logic]
use crate::shared::components::card::{card, CardProps};
use crate::shared::components::code_line::{code_line, CodeLineProps};
use crate::shared::components::doc_header::{doc_header, DocHeaderProps};
use crate::shared::components::example::{example, ExampleProps};
let tab = signal(0u32);
let open = signal(true);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Pane {
    Overview,
    Pricing,
    Team,
}

impl Pane {
    const ALL: [Pane; 3] = [Pane::Overview, Pane::Pricing, Pane::Team];

    fn slug(self) -> &'static str {
        match self {
            Pane::Overview => "overview",
            Pane::Pricing => "pricing",
            Pane::Team => "team",
        }
    }
}

impl Route for Pane {
    fn to_location(&self) -> Location {
        Location::from_segments(["navigation", self.slug()])
    }

    fn from_location(location: &Location) -> Option<Self> {
        match location.segments() {
            [section, slug] if section == "navigation" => {
                Pane::ALL.into_iter().find(|pane| pane.slug() == slug)
            }
            _ => None,
        }
    }
}

// Following the address makes this stack the app's history: the browser's back and forward, a `--location` argument and an Android link all land on a pane.
let panes = Navigator::new(Pane::Overview).follow_location();
let address = memo(move || location_format().format(&panes.location()));
let depth = memo(move || panes.depth());

[view]
col gap:20
    doc_header kicker:"NAVIGATION" title:"Tabs, accordion & address" desc:"tabs is a bound selected-index bar; pair it with reactive ifs to swap panels. accordion is an inline collapsible section that pushes its siblings as it opens. Both are components."
    example title:"tabs — a bound index; swap panels with reactive ifs"
        card gap:10
            tabs selected:$tab items:vec!["Overview","Pricing","Team"]
            if $tab == 0
                text "Overview — what the product does." font_size:14 color:$theme.ink
            if $tab == 1
                text "Pricing — plans and limits." font_size:14 color:$theme.ink
            if $tab == 2
                text "Team — who is behind it." font_size:14 color:$theme.ink
        code_line code:"tabs selected:$tab items:vec!['Overview','Pricing','Team']"
    example title:"accordion — a collapsible section, open bound to a signal"
        card gap:10
            accordion title:"Shipping details" open:$open
                text "Ships in 2–3 business days. Free over $50." font_size:14 color:$theme.muted
            text "open · {$open}" font_size:13 color:$theme.muted
        code_line code:"accordion title:'Shipping details' open:$open  >  …body…"
    example title:"follow_location — a page stack that is the app's address"
        card gap:10
            row gap:8 wrap
                button label:"Overview" on_press:(|| { panes.push(Pane::Overview) })
                button label:"Pricing" on_press:(|| { panes.push(Pane::Pricing) })
                button label:"Team" on_press:(|| { panes.push(Pane::Team) })
                button label:"Back" ghost on_press:(|| { navigate_back(); })
            text "{$address} · {$depth} deep" font_size:14 color:$theme.ink
            text "Open it again with --location {$address} on desktop or in a terminal, or reload the page on the web." font_size:13 color:$theme.muted
        code_line code:"let panes = Navigator::new(Pane::Overview).follow_location();"
